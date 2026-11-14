//! The pause menu: Esc in the world opens it instead of quitting (the original opens its
//! pause menu there [observed by the coordinator playing the original, 2026-10-09]). It is
//! a `PlayState::Menu` like the container and inventory menus, so the world, the game
//! clock and mouse look stop and the cursor is released (`crate::menu`).
//!
//! **The look is the original's** (`crate::start_menu`, the main menu shares it) [owner,
//! todo 5 item 8; observed: the owner's screenshots of the original's pause menu, its
//! settings pages and its quit question, C:\fnv-private\Pause Menu; start_menu.xml]: the
//! pause screen over the held world, the column of entries at the right (Continue, Save,
//! Load, Settings, Quit; the original's Help is left out), the highlight box round the
//! chosen one. A page opened from the column dims the column but for its own entry; a
//! click on that entry again closes the page (back to the column), and a click on Quit
//! while its question is up cancels it [owner 2026-10-16: the entries toggle their page].
//! Up/Down choose, Enter or Space take, a click takes, the pointer chooses the entry it
//! moves onto; Esc resumes; N saves, L opens the load page, S the settings, Q the quit
//! question.
//!
//! **Save** (N) saves to a new slot named `Save <n>` (one more than the highest such name
//! in the saves folder) through `crate::save::SaveRequest`; the line under the column
//! says when it is written. **The load page** (L) shows the chosen save's thumbnail and
//! details (player and level, place and game date, play time, when it was saved) over the
//! saves' rows, newest first (`fnv_save::list`), left of the column. Up/Down (W/S) or the
//! mouse wheel choose, the pointer chooses the row it moves onto, Enter, Space or a click
//! load it through `crate::save::LoadRequest` (nvforever starts again with `--load`); Esc,
//! Backspace or Back go back. Only the chosen save's thumbnail is read and decoded.
//! **Deleting a save** (2026-10-19; docs/design/save-load.md §6.1): Delete (or a click
//! on "Delete  Del)" right of the thumbnail) asks "Delete this save?" there, with "Yes
//! Y)" and "No  N)"; Y, Enter or a click on Yes deletes the chosen save's file from the
//! saves folder and lists the saves again; N, Esc, Backspace or a click on No keep it
//! [documented: the original's load and save menus have a Delete hint,
//! `load_and_save_menus.xml` `save_delete_text`, shown with a controller; design: the key,
//! the question and its place]. Log `pause menu: delete <name>?`, `pause menu: deleted
//! <path>`, `pause menu: delete cancelled`.
//!
//! **The settings page** (#1953; docs/design/settings-page.md) lists the rows of
//! [`crate::settings`] and the controls of `crate::bindings` in seven tabs, the original's
//! four categories first: Gameplay, Display, Audio, Controls, then Graphics, Tweaks and
//! Debug (`crate::settings_layout` says each row's tab and heading; a row it does not name
//! stays under the group it registered with) [design: #1953]. Left of the column, from the
//! rows' top, the rail lists Search, Changed and the tabs; the chosen tab's rows are in
//! the middle over the darkened band (from near the view's top, as wide as the room left of
//! the rail allows up to 1300 units and centred in it, the band clear of the rail:
//! [`ListBox::of`], #1210, #2377), under dim headings in the page's font (Gameplay: Game, Saving,
//! Dialogue, Camera; Display: Screen, Interface, Performance; Graphics: Picture,
//! Distance, World), which take a line of the window and the scroll bar but are never
//! chosen; each row with its value as the game is using it now, the chosen row's note
//! under them (three lines at most), Defaults and Back at the bottom right, the
//! version at the bottom left. The view model is `crate::settings_view`.
//!
//! **Where it opens** [design: #1953, the fork owner's request 2026-11-09]: Settings opens
//! the page where it was left this run, its tab (or the Changed list), chosen row and first
//! line ([`open_settings`]; left from the search, on what the search came from). The tab is
//! kept across runs too, as `[UI] SettingsTab=<tab>` of `NVForever.ini`, written when it
//! changes and read at start ([`SettingsTabStore`]; an unknown name opens the first tab;
//! an unattended run in the player's home neither reads nor writes it). Log `pause menu:
//! settings open on <tab>; selected <row>`, `settings page: [UI] SettingsTab=<tab> kept in
//! <ini>` / `read from <ini>`. A rail line under the pointer is drawn bright with the
//! highlight box's line round it, unfilled, and the focus sound plays once as the pointer
//! comes onto it (`pause menu: hover <line>`); the keys' line keeps its filled box.
//!
//! The keys [design: #1953]: Up/Down (or W/S) choose a row of the tab, round its ends,
//! passing the headings; Page Up/Page Down the first row under the heading before or after
//! (a controller's LB/RB; "Previous heading" and "Next heading" over the rows take a click
//! too); Tab the next tab, Shift+Tab or F1 the one before (a controller's RT and LT); F6
//! (Y) puts the keys on the rail, where Up/Down choose among Search, Changed, the tabs,
//! Defaults and Back, Enter (A) takes one and F6 gives the keys back to the rows; a click
//! on a tab shows it. Enter, Space, Left or Right flip a switch, as does a click on it; a
//! flip acts at once and is kept in `NVForever.ini` (`crate::settings`). A colour's red,
//! green and blue rows show only while the colour's list has Custom (`crate::ui_colour`);
//! a row chosen while hidden (a test aid's) gives the choice to its colour's row. A slider's row
//! shows a meter as the "How many?" prompt's (`crate::menu::look`: a ground at a quarter
//! of the colour, tick marks 8 apart as far as the value, an arrow at each end) and the
//! value as a percentage; Left/Right or the arrows move it a step, a press or drag on the
//! meter sets the value under the pointer, and it is kept in the INI too. Defaults puts
//! back every row as the command line left it before the INI was read
//! (`crate::settings::StartStates`; without it, as the page was first shown in this run);
//! on the Controls tab, the controls' keys (or buttons) and the tab's controller rows; on
//! the Changed list, the rows it lists. On the search it is the global Defaults.
//! Esc or Backspace go back.
//!
//! **The search** [design: #1953 point 5]: the rail's Search line (a click, or Enter on it
//! with the keys on the rail) or Ctrl+F anywhere on the page (not while a key is awaited,
//! a dropdown is open or the keep question is up: those have the keys) opens it. Its line
//! over the rows shows "Search:" and the text typed, the keys in it; what is typed (the
//! window's text, `KeyboardInput`, read every frame in `PreUpdate` so nothing typed before
//! comes in late) goes into it, Backspace takes the last letter off, and while it is open
//! the letters are its own: N, L, S, Q, W/S, A/D and Space do nothing else, and the game's
//! controls read no key (`crate::bindings::KeyboardCapture`; nvforever's hand-report form
//! holds the keys the same way while it is over the menu, [`FormOver`]). It finds every row of every
//! tab whose label or note (in the language shown or in English) or INI key holds each
//! word typed, any case (`crate::settings_view::search`), the Controls tab's own rows too
//! by their label, note and INI name; an empty text lists every row. The results are in
//! the page's order, under dim headings saying where each lives ("Gameplay > Dialogue");
//! a hidden row is listed as the row that stands for it (a colour's channel as its colour;
//! a controller's row, a binding or sensitivity of the device not in use, as Input Device,
//! so it is never edited through the device in use), its note naming what it stands for.
//! Down or Enter put the keys on the results, where they act as on the row's own tab
//! (Enter, Left/Right, a click; Page Up/Down from place to place) and the search stays
//! open; typing again goes back to the text. A frame's keys go in the order typed. Esc,
//! Back or the rail's Back closes it and shows what was listed before, the same row
//! chosen, and goes no further.
//! With a controller the rail's keys pass over Search, which says "Keyboard search": a
//! controller types nothing yet, but works the results a keyboard found.
//!
//! **The Changed list** [design: #1953 point 6]: every row of every tab set away from
//! what Defaults puts back (`crate::settings::changed_rows`: a switch by its state, a
//! slider as it keeps its value, a list by its text as the INI keeps it; hidden and
//! disabled rows too), and the Controls rows away from the game's (each control's key and
//! its controller button, the sensitivities, Invert Y, Rumble, the input device:
//! `crate::bindings::changed_controls`), under headings saying where each lives; "No changed
//! settings." when there is none. Defaults there puts back exactly the rows it lists, the
//! controls' each by itself with no swap (`crate::bindings::restore_rows`); a row that goes
//! back leaves the list and the choice moves to the row after it. Log `pause menu: changed
//! <n> row(s); keys <key>,…` as the list changes, `pause menu: changed defaults; keys …`.
//!
//! A list's row (the resolution, `crate::settings::ChoiceList`) shows its choice in a box
//! with a down arrow, as a dropdown [design; owner 2026-10-18]: Enter, Space or a click
//! open the list under it (over it near the bottom), the chosen one boxed; Up/Down (W/S),
//! the wheel and the pointer choose, Enter, Space or a click take, Esc, Backspace or a
//! click outside close it. Left/Right on the row take the choice before or after without
//! opening it. A choice that asks to be kept puts a question over the page: "Keep this
//! resolution?" with the seconds left and Keep / Revert (Left/Right/Up/Down choose, Enter
//! or Space take, Esc reverts, a click takes); unanswered it goes back by itself
//! (`crate::settings::KEEP_SECS`).
//! A list may show its choices by other words than their names and hold some out of reach
//! (`crate::settings::set_choice_labels`, `set_choice_unavailable`) [design:
//! docs/design/dlss-upscaling.md]: the box and the open list show the words; a choice out
//! of reach is drawn dim, the keys and the pointer still come onto it and the note under
//! the rows then gives its reason, but Enter, Space, A or a click leave the list open and
//! take nothing (`settings: <key> <name> not taken: <reason>`), and Left/Right on the row
//! pass over it. The test aids still name a choice by its name; one out of reach is
//! refused the same way.
//!
//! **The Controls tab** [design; owner 2026-10-16: keys can be mapped from the settings]
//! lists every control of `crate::bindings` with its key and mouse button. Up/Down (W/S)
//! choose; Enter, Space or a click on a row make it wait for its new key ("Press a
//! key..."; [`Listening`]): the next key or mouse button pressed is bound
//! (`crate::bindings::rebind`: a control that had it gets the old one), Esc cancels.
//! Delete clears the row's key; Defaults puts back the game's keys. Each change acts at
//! once and is kept in `[Controls]` of `NVForever.ini`. Its first row, Input
//! Device, switches between the keyboard and mouse and a controller (`crate::pad`);
//! with Controller chosen the list shows and rebinds each control's controller button
//! (kept in `[Controller]`), a row waiting for the next button pressed ("Press a
//! button..."), Start cancelling. After the controls, Invert Y turns the vertical look
//! upside down, and a slider as the settings page's sets how fast the view turns as a
//! percentage of the game's: Mouse Sensitivity with keyboard and mouse
//! (`fMouseSensitivity`), Stick Sensitivity with a controller (the right stick's look
//! speeds), each kept apart (Left/Right or the arrows a step of `crate::bindings::SENSITIVITY_STEPS`,
//! about a tenth apart, a press or drag on the logarithmic meter the value under the
//! pointer; `crate::bindings::SENSITIVITY_LEAST`..`SENSITIVITY_MOST`, 5%-300%), and under
//! it Vertical Sensitivity, each device's own, sets the vertical look's speed as a
//! percentage of the horizontal's on the same steps (100% as the game has it, #2201)
//! [design: owner 2026-11-03, #1451; #1632]; with a controller Rumble comes after, then
//! the registered controller rows, Button style and the aim assist's (`crate::pad`,
//! `crate::aim_assist`; shown with the controller only) [design: #1953].
//!
//! **Quit** asks first [observed: the original's question and its choices]: Main Menu
//! (nvforever starts again with `--menu`, [`MainMenuRequest`]), Exit Game, Cancel.
//!
//! **Mods** (issue #976, T11; `crate::tiles::mods`): with a mod's tiles in the start menu
//! (MCM's include), a Mods entry stands before Quit ([`ModsEntry`]). Its page draws the
//! mod's tiles from the tree over this menu, gives them the pointer and keys, and runs the
//! start menu's MenuMode scripts each frame; MCM's window opens with it. Esc, Backspace,
//! Back or MCM's own Back go back to the column [design: owner-review.md #34]. On by
//! default; `[UI] TileMenus=-StartMenu` turns it off.
//!
//! **What a menu holds** (docs/design/dialogue.md §8, Pausing). Every menu holds game
//! logic by its own means (`PlayState`, the runtime's menu mode). Whether the picture
//! stands still too is one switch for all of them, [`MenuPause`] (`nvforever
//! --menu-pause living|original`, default `living`): the dialogue screen, the inventory
//! and container menus, a script's message boxes and the menus scripts open [owner
//! decision 2026-10-10]. `original` pauses Bevy's virtual clock while such a menu is
//! open ([`freeze_picture`]), so animations, wind, water and clouds stand still.
//! **The pause menu is the exception: it always holds everything**, in both modes: the
//! picture (the virtual clock), scripts (`script.rs` does not tick under it) and sound
//! (`voice.rs` pauses the lines being said and resumes them).
//!
//! While the virtual clock stands still, what must go on reads [`AidTime`]: the timed
//! test aids, the menus' own timers and the scripts' frame time (MenuMode blocks and a
//! conversation's lines run in real time under an `original` menu). `nvforever
//! --run-seconds` counts real time then as well, so an unattended run always ends. The
//! test aids that act on the world can count from the scripts' first frame instead
//! ([`WorldAidClock`], `NVF_PLAY_AIDS_FROM_SCRIPTS=1`; which aids: `crate::test_aid`).
//!
//! Test aid `NVF_PLAY_TEST_PAUSE=<seconds>:<seconds>[,…]`: opens the pause menu at the
//! first time (as Esc) and resumes at the second (as Esc again, from either page), on
//! the aids' clock. `NVF_PLAY_TEST_SETTINGS=<seconds>:<key>[,…]`: at each time, with the
//! pause menu open, shows the settings page and flips the switch of that key (as a
//! click on its row); `<key>=<value>` sets a slider (e.g. `actor-distance=0.5`) or takes a
//! list's choice (`resolution=1024x768`, as a click on it in the open list); the key
//! `page` only shows the page, `controls` the Controls tab, and `bind:<control>=<input>`
//! (INI names, e.g. `bind:Jump=J`) binds a control there as Enter on its row and that key
//! would, `padbind:<control>=<button>` (`padbind:Jump=RB`) a controller button,
//! `device:<name>` (`device:Controller`) picks the input device, `rumble:1` or `rumble:0`
//! sets the controller's Rumble row, `invert:1` or `invert:0` the Invert Y row and
//! `automatic-input:0|1` toggles automatic device selection [design, #2200];
//! `sensitivity:<percent>` (`sensitivity:150`) the sensitivity slider of the device in use,
//! `vertical:<percent>` (`vertical:80`) its Vertical Sensitivity;
//! `tab:<name>` (`tab:Tweaks`, an empty tab too) shows a tab on its first row,
//! `select:<key>` chooses a row without changing it (a hidden one: the row that stands for
//! it), `layout` logs the tabs and each row's place (`settings layout: menu-pause ->
//! Gameplay > Dialogue`), `search:<text>` opens the search on that text with the keys on
//! its results (`pause menu: search "<text>": <n> result(s); keys …`) and `changed` the
//! Changed list [design: #1953]. An older list's key (`crate::settings::set_list_alias`:
//! nvforever's `fsr-upscaling=<choice>`) shows the row that took over and takes the older
//! row's choice there.
//! `NVF_PLAY_TEST_PAUSE_MENU=<seconds>:<action>[,…]`: with the pause menu open, `save` (the
//! Save button), `load` (the load page), `down`, `up`, `pick` (Enter on the load page, the
//! quit question, a settings row or its open dropdown, where `down` and `up` move too),
//! `back` (from the load or the settings page, the question or an open dropdown),
//! `defaults` (the settings page's Defaults), `wheel-down`, `wheel-up` (a notch of the
//! mouse wheel on the settings page or the Controls tab),
//! `keep` and `revert` (the keep question's answers),
//! `quit` (the question), `delete`, `yes` and `no` (the load page's delete question),
//! `mods` (the Mods page, when the column has its entry: [`ModsEntry`]),
//! `shot` (a screenshot `pause_<page>.png` in the working folder); on the settings page
//! `heading-down`, `heading-up` (Page Down, Page Up), `tab-down`, `tab-up` (Tab, F1),
//! `rail` (F6), `left`, `right` (the arrow keys), `pad:<button>` (`pad:RB`, `pad:D-Pad
//! Down`: the key the controller's button presses on this menu, `crate::pad::menu_key_at`)
//! and `inspect` (logs the tab, the chosen row, the lines shown, every shown row's key and
//! the words drawn) [design: #1953]. These press the keys they stand for, so they go the
//! keys' way. `search` (the rail's Search), `settings` (the column's Settings, as S: the
//! page where it was left), and as real input the next frame, through the
//! window's messages: `type:<text>` (each character's key pressed with its text, so the
//! keys' state has them too), `backspace`, `ctrl-f`; `point:<target>` (the pointer onto the
//! middle of what is drawn: `search`, `search-field`, `changed`, `defaults`, `back`,
//! `heading-up`, `heading-down`, `tab:<name>`, `row:<key>`, `choice:<n>`), `press` and
//! `release` (the left mouse button) [design: #1953].

use crate::interface::{bc_supported, u, Face, Picture, Pictures};
use crate::settings::{add_setting, Setting, Settings};
use crate::settings_layout::{ResolvedLayout, CONTROLS_TAB};
use crate::settings_view::{ControlRow, Found, RailItem, RowId, SettingsFocus, SettingsLayout, SettingsScope, ViewEntry, Visibility, NO_ROW_SOUND, SEARCH};
use fnv_util::words::{tr, tr_str};
use crate::start_menu::{color, highlight_colors, text as sm_text, Faces, Shade, BASE, COLUMN, ITEM, LINE, RIGHT, SUB_RIGHT, SUB_WIDTH};
use crate::{PlaySet, PlayState};
use bevy::ecs::system::SystemParam;
use bevy::image::CompressedImageFormatSupport;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::input::ButtonState;
use bevy::prelude::*;
use crate::ui_pointer::{Button, PointerState};
use bevy::ui::{ComputedNode, RelativeCursorPosition, UiGlobalTransform};
use fnv_save::{SaveHeader, SaveKind};
use nvf_assets::GameData;
use std::path::{Path, PathBuf};

/// What an open menu holds besides game logic (`nvforever --menu-pause`; the old name
/// `--dialogue-pause` still works). Not the pause menu's business: that one always holds
/// everything.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuPause {
    /// Only game logic stands still; the picture lives on.
    #[default]
    Living,
    /// The virtual clock stops: animations, wind, water and sky stand still too.
    Original,
}

/// Whether the picture stands still: always under the pause menu and under a loading
/// screen (`crate::loading`: nothing happens in the world behind it); under any other
/// menu (every state but `Playing`) with [`MenuPause::Original`].
///
/// The loading screen's own pause of the clock once lasted a frame in `living`: the pause
/// menu or the main menu closing under it (a load from them) cleared [`Paused`], so this
/// let the clock run again, and the world behind the screen went on while the next run
/// loaded the save — from the main menu, the new game's opening in Doc Mitchell's house,
/// heard as his footsteps and his sitting down [owner 2026-10-23].
pub fn picture_held(mode: MenuPause, paused: bool, state: PlayState) -> bool {
    paused || state == PlayState::Loading || (mode == MenuPause::Original && state != PlayState::Playing)
}

/// Real time spent while the virtual clock stood still, and this frame's share of it.
#[derive(Resource, Default, Debug)]
pub struct PlayClock {
    stopped: f32,
    frame: Option<f32>,
}

impl PlayClock {
    /// Real seconds spent so far while the virtual clock stood still.
    pub fn stopped_secs(&self) -> f32 {
        self.stopped
    }

    /// A clock whose frame stood still for `secs` of real time (tests).
    #[cfg(test)]
    pub(crate) fn frozen_frame(secs: f32) -> PlayClock {
        PlayClock { stopped: 0.0, frame: Some(secs) }
    }
}

/// The clock of what goes on while a menu holds the picture: timed test aids, menu
/// timers, the scripts' frame time.
#[derive(SystemParam)]
pub struct AidTime<'w> {
    time: Res<'w, Time>,
    clock: Res<'w, PlayClock>,
    world: Option<Res<'w, WorldAidClock>>,
}

impl AidTime<'_> {
    /// Seconds since the start: the virtual clock's, plus the real time it stood still.
    pub fn elapsed_secs(&self) -> f32 {
        self.time.elapsed_secs() + self.clock.stopped
    }

    /// The clock of the test aids that act on the world ([`WorldAidClock`]): the aids'
    /// clock, or, with `NVF_PLAY_AIDS_FROM_SCRIPTS=1`, `None` until the scripts first run
    /// and then the aids' clock less what the scripts' late start held it back.
    pub fn world_secs(&self) -> Option<f32> {
        let now = self.elapsed_secs();
        self.world.as_deref().map_or(Some(now), |w| w.at(now))
    }

    /// The frame's time: the virtual clock's, or real time (at most 0.25 s, the virtual
    /// clock's own cap) while it stands still.
    pub fn delta_secs_f64(&self) -> f64 {
        self.clock.frame.map_or_else(|| self.time.delta_secs_f64(), f64::from)
    }
}

/// When the scripts start on a machine that is not busy, at the latest (seconds on the
/// aids' clock) [observed: the desktop's smoke logs of 2026-10-15/16 load the script data
/// in 4.2–6.5 s ("scripts ready in"); the scripts run from the first frame after it in
/// which the player stands in the world; busy runs started them at 8.7 s and 9.5 s].
/// Up to this the world aids' clock is not held back, so a scenario's times mean what
/// they meant before the switch.
pub const SCRIPTS_START_SECONDS: f32 = 6.5;

/// `NVF_PLAY_AIDS_FROM_SCRIPTS=1` (the smoke runner sets it for `--play` runs): the test
/// aids that act on the world count from when the scripts first run, not from the start,
/// so on a busy machine (the script data loads late) they do not run before the world
/// can take them [design; the console then answered "Scripts are not running", docs/
/// outstanding.md §5].
///
/// Such an aid waits until the scripts first run (`crate::script`; with scripts off,
/// until the player first stands in the world). From then its clock is the aids' clock
/// ([`AidTime::elapsed_secs`]) less what the late start held it back: the start's time
/// past [`SCRIPTS_START_SECONDS`]. A step due before then runs in the first frame the
/// scripts run; every later step keeps its time on an unbusy machine and its distance
/// from the others on a busy one. The script steps are still moved on their own when
/// the scripts start after the first one's time (`script_events::delay_late_aids`),
/// now by at most `SCRIPTS_START_SECONDS` less that time: a busy run then looks like
/// the slowest unbusy one [design; follower-boone failed when only its script steps
/// came 7.2 s late, 2026-10-17]. Aids that also work before the world (the main menu,
/// the intro, the pause menu and its settings page, an audio device reopen) keep the
/// aids' clock. `nvforever --run-seconds` is lengthened by the held-back seconds, so
/// the last steps still run.
///
/// Without the switch the world clock is the aids' clock, as before.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct WorldAidClock {
    from_scripts: bool,
    /// The seconds held back, once the scripts ran.
    held: Option<f32>,
}

impl WorldAidClock {
    pub fn new(from_scripts: bool) -> WorldAidClock {
        WorldAidClock { from_scripts, held: None }
    }

    pub fn from_env() -> WorldAidClock {
        let on = std::env::var("NVF_PLAY_AIDS_FROM_SCRIPTS").is_ok_and(|v| !matches!(v.trim(), "" | "0" | "off" | "false"));
        WorldAidClock::new(on)
    }

    /// The world aids' clock at `now` on the aids' clock (`None`: not started yet).
    pub fn at(&self, now: f32) -> Option<f32> {
        if !self.from_scripts {
            return Some(now);
        }
        self.held.map(|h| now - h)
    }

    /// The seconds the world aids' clock was held back (0 without the switch or before
    /// the start).
    pub fn held_secs(&self) -> f32 {
        self.held.unwrap_or(0.0)
    }

    /// The scripts first ran at `now` (the aids' clock): starts the clock and returns the
    /// seconds it is held back, the first time and with the switch only.
    pub fn start(&mut self, now: f32) -> Option<f32> {
        if !self.from_scripts || self.held.is_some() {
            return None;
        }
        let held = (now - SCRIPTS_START_SECONDS).max(0.0);
        self.held = Some(held);
        Some(held)
    }
}

/// The virtual clock is not paused and yet runs at no speed: real time is to be counted
/// for it by the clocks that must not stop with the world ([`tick_play_clock`], nvforever's
/// `--run-seconds`).
pub fn stands_still(paused: bool, relative_speed: f32) -> bool {
    !paused && relative_speed <= 0.0
}

/// The start's loading screen (`crate::loading::StartCover`) counts on neither clock: the
/// test aids count from when it comes down, as if the run began then.
pub(crate) fn tick_play_clock(virt: Res<Time<Virtual>>, real: Res<Time<Real>>, loading: Option<Res<crate::loading::Loading>>, mut clock: ResMut<PlayClock>) {
    clock.frame = virt.is_paused().then(|| real.delta_secs().min(0.25));
    if loading.is_some_and(|l| l.starting()) {
        return;
    }
    clock.stopped += clock.frame.unwrap_or(0.0);
    // A clock running at no speed (the classic VATS style's full stop, `crate::vats_mode`)
    // stands still too, but nothing takes the picture: the frame's time stays the
    // world's, none, and only the aids' clock goes on, so a timed key still comes.
    if stands_still(virt.is_paused(), virt.relative_speed()) {
        clock.stopped += real.delta_secs().min(0.25);
    }
}

/// Pause the virtual clock while the picture is held ([`picture_held`]), or while the
/// console's free camera holds the world (`crate::free_cam`, mode 1), and let it run
/// again after. Nothing else pauses it.
fn freeze_picture(mode: Res<MenuPause>, paused: Res<Paused>, state: Res<State<PlayState>>, free_cam: Option<Res<crate::free_cam::FreeCam>>, mut clock: ResMut<Time<Virtual>>) {
    let held_by_cam = free_cam.is_some_and(|f| f.holds_world());
    let stop = picture_held(*mode, paused.0, *state.get()) || held_by_cam;
    if clock.is_paused() != stop {
        let why = if paused.0 {
            "the pause menu"
        } else if *state.get() == PlayState::Loading {
            "a loading screen"
        } else if held_by_cam {
            "the free camera"
        } else {
            "--menu-pause original"
        };
        if stop {
            info!("menu pause: the picture stands still ({why})");
            clock.pause();
        } else {
            info!("menu pause: the picture runs again");
            clock.unpause();
        }
    }
}

/// `NVF_PLAY_TEST_PAUSE`: when to open the pause menu and when to resume;
/// `NVF_PLAY_TEST_SETTINGS`: when to flip which switch of the settings page.
#[derive(Resource, Default, Debug, PartialEq)]
struct PauseAid {
    open: Vec<f32>,
    resume: Vec<f32>,
    switches: Vec<(f32, String)>,
    /// `NVF_PLAY_TEST_PAUSE_MENU`: save, load, down, up, pick, back.
    actions: Vec<(f32, String)>,
}

/// Parse `<open seconds>:<resume seconds>[,…]`.
fn parse_pause_aid(v: &str) -> Result<PauseAid, String> {
    let mut aid = PauseAid::default();
    for item in v.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (a, b) = item.split_once(':').ok_or_else(|| format!("{item:?}: expected <open seconds>:<resume seconds>"))?;
        let num = |s: &str| s.trim().parse::<f32>().map_err(|_| format!("{item:?}: bad seconds {s:?}"));
        let (a, b) = (num(a)?, num(b)?);
        if b <= a {
            return Err(format!("{item:?}: the menu resumes after it opens"));
        }
        aid.open.push(a);
        aid.resume.push(b);
    }
    Ok(aid)
}

/// Parse `<seconds>:<key>[,…]`.
fn parse_settings_aid(v: &str) -> Result<Vec<(f32, String)>, String> {
    v.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|item| {
            let (t, key) = item.split_once(':').ok_or_else(|| format!("{item:?}: expected <seconds>:<key>"))?;
            let t = t.trim().parse::<f32>().map_err(|_| format!("{item:?}: bad seconds {t:?}"))?;
            Ok((t, key.trim().to_string()))
        })
        .collect()
}

/// Whether a time of `due` has come (each fires once).
fn fire(due: &mut Vec<f32>, now: f32) -> bool {
    let before = due.len();
    due.retain(|&t| t > now);
    due.len() < before
}

/// Whether the pause menu is the open menu.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Paused(pub bool);

/// A form of nvforever's own is up over the pause menu (the hand report,
/// nvforever's `reports/form.rs`, player-reports.md §3.3): the pause menu takes no key or
/// click while it is, and a text field has the keyboard ([`crate::bindings::KeyboardCapture`]).
/// The form clears it the frame after it closes, so the key that closed it does nothing
/// here.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormOver(pub bool);

/// Where a form over the pause menu stands among the interface's roots: above the pause
/// menu (`PauseRoot`'s 110 and its boxes' 111-113) and the tile menus over it (Mods, 115),
/// under the loading screen (200).
pub const FORM_Z: i32 = 180;
// Over the pause menu (110-113) and its Mods tiles (115), under the loading screen (200).
const _: () = assert!(FORM_Z > 115 && FORM_Z < 200);

/// How much of the light under a form its full-screen shade lets through, as the alpha of
/// black. The interface blends in linear light, so an alpha keeps `1 - alpha` of the light and
/// that reads far brighter than it sounds: 0.7 left the menu under it at 58 % of its
/// brightness on screen [observed: #2250's pictures, the pause menu's amber (255, 182, 66)
/// read (149, 105, 35) beside the dialog]. 0.93 leaves about 29 % on screen [design].
pub const FORM_SHADE: f32 = 0.93;

/// The full-screen shade under a form ([`FORM_SHADE`]).
pub fn form_shade() -> BackgroundColor {
    BackgroundColor(Color::srgba(0.0, 0.0, 0.0, FORM_SHADE))
}

/// A form's panel: opaque, so no text of the menu under it shows through [design: an alpha of
/// 0.95 still let it show at about 14 % of its brightness over the shade, #2250's pictures].
pub fn form_panel() -> BackgroundColor {
    BackgroundColor(Color::srgb(0.02, 0.02, 0.02))
}

/// What share of a colour's value on screen is left under black of `alpha`, blending in linear
/// light (the sRGB encoding of `1 - alpha`).
pub fn shown_under(alpha: f32) -> f32 {
    let linear = (1.0 - alpha).clamp(0.0, 1.0);
    if linear <= 0.003_130_8 { linear * 12.92 } else { 1.055 * linear.powf(1.0 / 2.4) - 0.055 }
}

/// The "Report a bug" entry at the pause menu's bottom left, over the version (owner
/// 2026-11-12): `shown` when nvforever's hand-report form is there to open
/// (`reports/form.rs` sets it); a click sets `asked`, which the form takes. Only over a
/// game, never under the main menu.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportEntry {
    pub shown: bool,
    pub asked: bool,
}

/// The report entry's words [design: NVForever's own].
const REPORT_LABEL: &str = "Report a bug";

fn no_form(form: Option<Res<FormOver>>) -> bool {
    !form.is_some_and(|f| f.0)
}

/// The page of the pause menu that is up; on the settings page, the row the keys are on.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    #[default]
    Main,
    /// The settings page: the chosen row (`None`: the first of the tab, or none on an empty
    /// one), on the tab or list [`PauseView`] says (`crate::settings_view`).
    Settings {
        selected: Option<RowId>,
    },
    /// The load page; the chosen save (index in [`LoadList`]).
    Load {
        selected: usize,
    },
    /// The Mods page: a mod's page of `StartMenu` drawn from its tiles (MCM's), with the
    /// start menu's MenuMode scripts running (issue #976, T11; [`ModsEntry`]).
    Mods,
}

impl Page {
    /// On the settings page, the chosen row's number (`crate::ui_sounds` hears it move;
    /// [`RowId::sound_id`], [`NO_ROW_SOUND`] on an empty tab).
    pub(crate) fn settings_row(self) -> Option<usize> {
        match self {
            Page::Main | Page::Load { .. } | Page::Mods => None,
            Page::Settings { selected } => Some(selected.map_or(NO_ROW_SOUND, RowId::sound_id)),
        }
    }

    /// Whether `label`, an entry of the column, is the one this page stands for: a click
    /// on it closes the page again [owner 2026-10-16].
    fn opened_by(self, label: &str) -> bool {
        match self {
            Page::Main => false,
            Page::Settings { .. } => label.starts_with("Settings"),
            Page::Load { .. } => label.starts_with("Load"),
            Page::Mods => label.starts_with(MODS_LABEL),
        }
    }
}

/// A control of the Controls tab waiting for its new key or button, and the device it
/// was asked with (Enter on its row); `None`: none waits. `crate::pad` presses no menu key
/// for the controller's buttons meanwhile.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Listening(pub Option<(crate::bindings::Control, crate::bindings::InputDevice)>);

/// Whether the column has a Mods entry this frame: with the start menu's switch on (the
/// default, `crate::tiles::vanilla::TileMenus::for_run`), a mod added tiles to `StartMenu`
/// (MCM's include) [design: owner-review.md #34, "a Mods entry appears only when a mod
/// added one"; ui-tile-tree.md §7]. Written by `crate::tiles` each frame.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModsEntry(pub bool);

/// Whether the Mods page is up: the pause menu open on [`Page::Mods`].
pub fn mods_open(paused: &Paused, page: &Page) -> bool {
    paused.0 && *page == Page::Mods
}

/// The Mods entry's label [design: NVForever's word].
const MODS_LABEL: &str = "Mods";

/// The saves the load page lists, newest first, and the chosen one's thumbnail.
#[derive(Resource, Default)]
struct LoadList {
    saves: Vec<(PathBuf, SaveHeader)>,
    /// The thumbnail of the save at this index (None: it has none or it did not decode).
    thumb: Option<(usize, Option<Handle<Image>>)>,
}

impl LoadList {
    fn open(&mut self, dir: Option<&Path>) {
        self.saves = dir.map(fnv_save::list).unwrap_or_default();
        self.thumb = None;
        let names: Vec<String> = self.saves.iter().take(8).map(|(p, h)| format!("{:?} ({})", stem(p), kind_name(h.kind))).collect();
        info!("pause menu: load page: {} save(s), newest first: {}", self.saves.len(), names.join(", "));
    }

    /// Read and decode the thumbnail of save `i` unless it is the one held.
    fn ensure_thumb(&mut self, i: usize, images: &mut Assets<Image>) {
        if self.thumb.as_ref().is_some_and(|(t, _)| *t == i) {
            return;
        }
        let Some((path, _)) = self.saves.get(i) else { return };
        let decoded = fnv_save::read_header(path).ok().and_then(|f| crate::save::decode_thumbnail(&f.shot));
        let handle = decoded.and_then(|(w, h, rgba)| {
            info!("pause menu: load page row {i} {:?}: thumbnail {w}x{h}", stem(path));
            // A broken save's picture is left out rather than handed to the GPU.
            let format = bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb;
            let mut img = nvf_assets::texture::checked_image(w, h, 1, format, rgba).map_err(|e| warn!("pause menu: load page row {i}: thumbnail: {e}")).ok()?;
            img.asset_usage = bevy::asset::RenderAssetUsages::default();
            Some(images.add(img))
        });
        if handle.is_none() {
            info!("pause menu: load page row {i} {:?}: no thumbnail", stem(path));
        }
        self.thumb = Some((i, handle));
    }
}

/// The line under the main page's buttons: how the save asked from this menu went.
#[derive(Resource, Default)]
struct PauseStatus {
    waiting_for: Option<PathBuf>,
    line: String,
}

#[derive(Component)]
struct PauseStatusText;

/// What the Save button and the load page use.
#[derive(SystemParam)]
struct SaveMenu<'w> {
    list: ResMut<'w, LoadList>,
    images: ResMut<'w, Assets<Image>>,
    cfg: Res<'w, crate::save::SaveConfig>,
    saves: MessageWriter<'w, crate::save::SaveRequest>,
    loads: MessageWriter<'w, crate::save::LoadRequest>,
    status: ResMut<'w, PauseStatus>,
    motion: Option<Res<'w, AccumulatedMouseMotion>>,
    scroll: Option<Res<'w, AccumulatedMouseScroll>>,
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

fn kind_name(k: SaveKind) -> &'static str {
    match k {
        SaveKind::Manual => "manual",
        SaveKind::Quick => "quick",
        SaveKind::Auto => "auto",
        SaveKind::System => "system",
    }
}

/// Delete a save: its file, which holds the header, the thumbnail and the game
/// (`fnv_save`; docs/design/save-load.md §6.1), and the `.bak` a quick or auto slot keeps
/// of the one before. Only files of the saves' own extension are removed.
fn delete_save(path: &Path) -> std::io::Result<()> {
    if path.extension().is_none_or(|x| x != fnv_save::EXTENSION) {
        return Err(std::io::Error::other("not a save file"));
    }
    std::fs::remove_file(path)?;
    let _ = std::fs::remove_file(path.with_extension(format!("{}.bak", fnv_save::EXTENSION)));
    Ok(())
}

/// The name of a new save in the saves folder ([`next_save_name`] of the files there).
pub fn new_save_name(cfg: &crate::save::SaveConfig) -> String {
    let stems: Vec<String> = cfg.dir.as_deref().and_then(|d| std::fs::read_dir(d).ok()).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == fnv_save::EXTENSION)).map(|p| stem(&p)).collect();
    next_save_name(stems.iter().map(String::as_str))
}

/// The name of a new save: `Save <n>`, one more than the highest such name in `stems`.
fn next_save_name<'a>(stems: impl IntoIterator<Item = &'a str>) -> String {
    let n = stems.into_iter().filter_map(|s| s.strip_prefix("Save ")).filter_map(|n| n.parse::<u32>().ok()).max().unwrap_or(0);
    format!("Save {}", n + 1)
}

/// Play time as hours and minutes.
fn play_time(seconds: f64) -> String {
    let m = (seconds.max(0.0) / 60.0) as u64;
    if m < 60 {
        format!("{m} min")
    } else {
        format!("{} h {:02} min", m / 60, m % 60)
    }
}

/// How long ago a save was made (real time).
fn saved_ago(saved_unix: u64, now_unix: u64) -> String {
    let s = now_unix.saturating_sub(saved_unix);
    match s {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", s / 60),
        3600..86400 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86400),
    }
}

#[derive(Component)]
struct PauseRoot;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum PauseButton {
    /// An entry of the column at the right (its index).
    Column(usize),
    /// A line of the settings page's rail: Search, Changed or a tab.
    Rail(RailItem),
    /// The search's line over its results: a click puts the keys in its text.
    SearchField,
    /// A row of the settings page.
    Row(RowId),
    /// "Previous heading" (-1) or "Next heading" (1) over the settings page's rows: as Page
    /// Up and Page Down [design: #1953].
    HeadingStep(i32),
    /// A save's row on the load page (its index in [`LoadList`]).
    LoadRow(usize),
    /// A slider's arrow: its row and the steps it moves (-1 or 1).
    SliderStep(RowId, i32),
    /// A slider's meter (its row): a press or drag sets the value under the pointer.
    Meter(RowId),
    /// The scroll bar's arrow: the rows it moves the list (-1 or 1).
    ScrollStep(i32),
    /// The scroll bar's track: a press or drag puts the marker under the pointer.
    ScrollTrack,
    /// The settings page's Defaults.
    Defaults,
    /// From the settings or load page back to the first page.
    Back,
    /// A choice of the quit question (its index in [`CONFIRM`]).
    Confirm(usize),
    /// A choice in a list's open dropdown: its row and the choice's index.
    Choice(usize, usize),
    /// An answer of the keep question (its index in [`KEEP_ANSWERS`]).
    Keep(usize),
    /// The load page's "Delete  Del)".
    Delete,
    /// An answer of the delete question: yes or no.
    DeleteAnswer(bool),
    /// "Report a bug" at the bottom left ([`ReportEntry`]).
    Report,
}

/// A row's value, kept as text too (tests read it).
#[derive(Component)]
struct SwitchState(#[allow(dead_code)] RowId);

/// A heading's line, kept as text too (tests read it).
#[derive(Component)]
struct HeadingLine;

/// The chosen switch's note, kept as text too (tests read it).
#[derive(Component)]
struct SwitchNote;

/// The column's entries in the world [observed: the owner's screenshots of the original's
/// pause menu: Continue, Save, Load, Settings, Help, Quit; Help is left out, NVForever has
/// no help pages].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Continue,
    Save,
    Load,
    Settings,
    /// Only with [`ModsEntry`], before Quit [design].
    Mods,
    Quit,
}

/// The column's entries, with the Mods entry when `mods` ([`ModsEntry`]).
fn entries(mods: bool) -> Vec<Entry> {
    let mut v = vec![Entry::Continue, Entry::Save, Entry::Load, Entry::Settings];
    if mods {
        v.push(Entry::Mods);
    }
    v.push(Entry::Quit);
    v
}

impl Entry {
    fn label(self) -> &'static str {
        match self {
            Entry::Continue => "Continue",
            Entry::Save => "Save",
            Entry::Load => "Load",
            Entry::Settings => "Settings",
            Entry::Mods => MODS_LABEL,
            Entry::Quit => "Quit",
        }
    }
}

/// The quit question and its choices [observed: the owner's screenshot of the original's
/// quit question].
const WARNING: &str = "WARNING:  Any unsaved progress will be lost.";
const QUESTION: &str = "Are you sure you want to quit?";
const CONFIRM: [&str; 3] = ["Main Menu", "Exit Game", "Cancel"];
/// The keep question's answers [design].
const KEEP_ANSWERS: [&str; 2] = ["Keep", "Revert"];

/// What an empty tab shows, a search that finds nothing and an empty Changed list
/// [design: #1953].
const NO_ROWS: &str = "No settings here.";
const NO_RESULTS: &str = "No results.";
const NO_CHANGED: &str = "No changed settings.";
/// The rail's Search line while a controller works the controls: the search is typed, and
/// the controller's keys pass over it [design: #1953].
const KEYBOARD_SEARCH: &str = "Keyboard search";
/// The mark after the search's text while it has the keys [design].
const CARET: &str = "_";
/// The links over the rows to the heading before and after [design: #1953].
const HEADING_LINKS: [&str; 2] = ["Previous heading", "Next heading"];
/// A control's value while it waits for its new key.
const WAITING: &str = "Press a key...";
/// The same with a controller.
const WAITING_PAD: &str = "Press a button...";
/// The Controls tab's first row.
const DEVICE_ROW: &str = "Input Device";
// [design, #2200] NVForever's own label and help, translated by `tr`.
const AUTOMATIC_INPUT_ROW: &str = "Automatic Input Switching";
const AUTOMATIC_INPUT_NOTE: &str = "On: a controller button, stick or trigger selects Controller; a key or mouse click selects Keyboard and Mouse (mouse motion does not). Off: choose Input Device yourself.";
/// The Controls tab's last row with a controller chosen (`crate::rumble`).
const RUMBLE_ROW: &str = "Rumble";
/// The Controls tab's row after the controls (`bInvertYValues`).
const INVERT_ROW: &str = "Invert Y";
/// The Controls tab's slider after Invert Y, with keyboard and mouse and with a controller.
const SENSITIVITY_ROWS: [&str; 2] = ["Mouse Sensitivity", "Stick Sensitivity"];
/// The Controls tab's slider under the sensitivity, each device's own (#2201).
const VERTICAL_ROW: &str = "Vertical Sensitivity";

/// The settings page's rows: each this high [observed: 39 of 720 pixels], the list this
/// wide and its left edge this far left of the view's middle, the switch's state centred
/// this far into a row [design: the original's list is 520 wide and centred, its values at
/// 360; NVForever's labels are longer]. How many lines show at once is the layout's
/// ([`window_lines`]): the original shows nine [documented: `options_container`
/// `_number_of_visible_items` 9], NVForever's page as many as fit over the note [design:
/// #2377].
const OPTION_ROW: f32 = 52.0;
/// The scroll bar right of the settings page's rows when not all of them fit, 32 wide
/// with 16-high arrows at its ends, its marker 6 wide, 12 in [design: as the barter
/// menu's (`crate::menu::look`), from the game's `interface\shared\scrollbar` pictures].
const BAR_W: f32 = 32.0;
const BAR_ARROW: f32 = 16.0;
const BAR_GAP: f32 = 4.0;
const BAR_MARKER_MIN: f32 = 32.0;
const BAR_PICTURES: [&str; 3] = ["interface\\shared\\scrollbar\\arrow_up.dds", "interface\\shared\\scrollbar\\arrow_down.dds", "interface\\shared\\scrollbar\\vert_marker.dds"];
/// Mouse wheel movement counted in pixels (a touchpad): this many make a notch.
const WHEEL_PIXELS: f32 = 20.0;
/// The settings list's band starts this far from the view's left edge where the list is as
/// wide as the room allows, and the list is never wider than [`OPTIONS_MAX`] (a wide view
/// centres it in the room) [design: #2377].
const LEFT_MARGIN: f32 = 60.0;
const OPTIONS_MAX: f32 = 1300.0;
/// The row the in-row places below were laid out in: the list's width before #2377. Each
/// place is kept as its distance from the row's right end ([`RowX`]), so a row of this width
/// is drawn as before and a wider row gives its extra room to the label [design: #2377].
const ROW_BASE: f32 = 820.0;
/// The least room between a row's label and its value; a label keeps at least
/// [`LABEL_MIN`] where a value is wide (a long reason or binding), and text that still does
/// not fit is cut with [`CUT_MARK`] at its measured width instead of running into its
/// neighbour [design: #2377].
const LABEL_GAP: f32 = 16.0;
const LABEL_MIN: f32 = 160.0;
const CUT_MARK: &str = "...";
/// A value's text keeps this far from the row's right end, and a slider's words this far
/// from its right arrow [design: #2377].
const VALUE_PAD: f32 = 6.0;
const ARROW_PAD: f32 = 4.0;
/// A slider's meter gives room to wide value words down to this width [design: #2377].
const METER_MIN: f32 = 100.0;
/// The settings page's rows' top: higher than the column's (360 with the pause menu's
/// five entries), where nine rows and the note under them ran off the bottom [design:
/// #1210]. The rail starts there too: Search, Changed and seven tabs are nine lines
/// [design: #1953].
const OPTIONS_TOP: f32 = 120.0;
/// The heading links' line over the rows [design: #1953].
const LINKS_TOP: f32 = OPTIONS_TOP - ITEM - 8.0;
/// The note under the rows: its lines this far apart, the last one above the version's
/// line [design].
const NOTE_STEP: f32 = 26.0;
const NOTE_BOTTOM: f32 = 900.0;
/// The note's room under the window, in lines: two for a row's note and one for what it
/// stands for or a choice's reason (#2377) [design]; never more are drawn.
const NOTE_LINES: usize = 3;
/// A row's own note keeps to this many of the note's lines, the last of [`NOTE_LINES`] left
/// for what it stands for or a choice's reason [design: #2377].
const NOTE_ROW_LINES: usize = 2;
/// The rows a stand-in's note names before "and N more" ([`note_of`]) [design: #2377].
const STAND_IN_NAMES: usize = 3;
/// The top of "Report a bug" at the bottom left ([`report_entry`]), over the version's line:
/// the window and its note end above it, so the note never runs into it where the list
/// starts near the view's left edge [design: #2377].
const REPORT_TOP: f32 = 960.0 - 60.0 - ITEM;
/// The room between the window's last line and the note [design].
const NOTE_GAP: f32 = 20.0;
/// The room between the band's right edge and the categories [design: #1210, the scroll
/// bar and band ran under them at 16:9].
const TABS_GAP: f32 = 20.0;
/// The view's width in interface units when no window says (16:9).
const VIEW_W: f32 = 960.0 * 16.0 / 9.0;
const VALUE_AT: f32 = 700.0;
/// A slider's meter in its row: its left edge, width and height, the arrows' left edges
/// and the value's centre [design: the "How many?" meter's height and arrows
/// (`crate::menu::look`), fitted into the row's right part].
const METER_LEFT: f32 = 445.0;
const METER_WIDTH: f32 = 240.0;
const METER_HEIGHT: f32 = 24.0;
const METER_ARROWS: [f32; 2] = [420.0, 692.0];
const METER_ARROW: Vec2 = Vec2::new(17.5, 35.0);
const PERCENT_AT: f32 = 765.0;
/// A list's box in its row (its left edge and width, from the slider's left arrow to the
/// row's end), the open list's rows shown at once and their height [design]. The places
/// above and this one are in a [`ROW_BASE`] row; [`RowX`] keeps them at their distance from
/// the row's right end.
const LIST_LEFT: f32 = 420.0;
const LIST_WIDTH: f32 = 386.0;
const DROP_ROWS: usize = 8;
const DROP_ITEM: f32 = 44.0;
/// A tick of the meter: 8 apart, the picture's top 8 × 20 texels stretched to the meter's
/// height (as the "How many?" meter's [observed]).
const METER_TICK: f32 = 8.0;
const TICK_TEXTURE: &str = "textures\\interface\\hud\\hud_tick_mark.dds";
const ARROW_TEXTURES: [&str; 2] = ["textures\\interface\\main\\main_arrow_left.dds", "textures\\interface\\main\\main_arrow_right.dds"];
/// The darkened band behind a page's list [documented: `container_background`
/// `fuzzy.dds` at alpha 160 the height of the screen, a little wider than the list].
const BAND: &str = "textures\\interface\\shared\\background\\fuzzy.dds";
const BAND_ALPHA: f32 = 160.0 / 255.0;
const BAND_MARGIN: f32 = 30.0;
/// The black under the band [design: the band alone left the main menu's logo and
/// pictures showing through, observed 2026-10-16].
const BAND_SHEET: f32 = 0.55;
/// The load page: rows shown at once, the panel's width, its rows' top, the thumbnail and
/// the details' lines [documented: `saveload_container` width 650, y 521; the player's
/// name, level and play time at y 347, 388, 429; the thumbnail above the name; design:
/// eight rows].
const LOAD_ROWS: usize = 8;
const LOAD_WIDTH: f32 = 650.0;
const LOAD_TOP: f32 = 521.0;
const THUMB_DRAWN: Vec2 = Vec2::new(448.0, 252.0);
const DETAILS_TOP: f32 = 347.0;
const DETAILS_STEP: f32 = 41.0;
/// "Delete  Del)" and its question: right of the thumbnail, in what the panel leaves
/// [design].
const DELETE_WIDTH: f32 = LOAD_WIDTH - THUMB_DRAWN.x - 20.0;
/// The page's Defaults and Back at the bottom right [observed: the owner's screenshots].
const BACK_TOP: f32 = 960.0 - 30.0 - ITEM;
/// The pause screen behind the menu in the world: the original shows a model,
/// `meshes\interface\pausescreen\pausescreen01.nif`, whose textures are these [observed:
/// the file's texture names]; drawn here flat over the held picture, the half picture
/// at the left and mirrored at the right [hypothesis: the model's two slides], the world
/// behind darkened [design]. The model's glare (`loading_glare.dds`) is left out: drawn
/// over the picture, not added to it, it came out a dark slab [observed 2026-10-16].
const SCREEN: &str = "textures\\interface\\pause\\pausescreen01.dds";
const SCREEN_ALPHA: f32 = 0.82;
/// How much of the pause screen picture lies past the view's edges, as parts of the drawn
/// picture: its outer side (the left of the half picture, mirrored at the right), its top
/// and its bottom, so the frame's edge meets the view's [observed: the owner's screenshot
/// of 2026-10-16 marking where the edge belongs, at about 7% in from the sides, 5% from
/// the top and 9% from the bottom of the stretched picture].
const SCREEN_CROP_SIDE: f32 = 0.147;
const SCREEN_CROP_TOP: f32 = 0.051;
const SCREEN_CROP_BOTTOM: f32 = 0.088;
const WORLD_SHADE: f32 = 0.35;

/// The settings page's group of switches about conversations, sound and menus.
pub const GROUP_MENUS: &str = "Gameplay";

/// The fonts and pictures of the pause menu, read once.
#[derive(Resource)]
struct PauseLook {
    faces: Faces,
    band: Option<Handle<Image>>,
    screen: Option<Handle<Image>>,
    /// The sliders' tick mark and arrows (left, right).
    tick: Option<Handle<Image>>,
    arrows: [Option<Handle<Image>>; 2],
    /// The scroll bar's arrows (up, down) and marker.
    bar: [Option<Picture>; 3],
}

impl PauseLook {
    fn load(world: &mut World) -> PauseLook {
        let bc = bc_supported(world.get_resource::<CompressedImageFormatSupport>());
        world.resource_scope(|world, mut images: Mut<Assets<Image>>| {
            let data = world.get_resource::<GameData>();
            let pic = |path: &str, images: &mut Assets<Image>| data.and_then(|d| crate::interface::texture(path, d, images, bc)).map(|(h, _)| h);
            let look = PauseLook {
                faces: Faces::load(data, &mut images, bc),
                band: pic(BAND, &mut images),
                screen: pic(SCREEN, &mut images),
                tick: pic(TICK_TEXTURE, &mut images),
                arrows: ARROW_TEXTURES.map(|a| pic(a, &mut images)),
                bar: {
                    let mut pictures = Pictures::load(data);
                    BAR_PICTURES.map(|b| pictures.picture(b, data, &mut images, bc))
                },
            };
            info!("pause menu: the original's look (band {}, pause screen {})", look.band.is_some(), look.screen.is_some());
            look
        })
    }

    #[cfg(test)]
    fn plain() -> PauseLook {
        PauseLook { faces: Faces::plain(), band: None, screen: None, tick: None, arrows: [None, None], bar: [None, None, None] }
    }
}

/// What the keys have on the first page and in the quit question, and what Defaults goes
/// back to (taken when the settings page is first shown).
#[derive(Resource, Default)]
struct PauseView {
    main_selected: usize,
    confirm: Option<usize>,
    defaults: Option<Vec<f32>>,
    drawn: Option<Drawn>,
    /// The slider whose meter is being dragged (pressed and held).
    dragging: Option<RowId>,
    /// The settings page's first line shown (headings count), counted from its tab's
    /// first; on the load page, the saves' first row shown.
    scroll: usize,
    /// The settings page's window's lines ([`window_lines`]), set before the keys and the
    /// pointer are read each frame; 0 until then (taken as [`window_lines`]).
    lines: usize,
    /// What the settings page lists: a tab (or the search, the Changed list).
    scope: SettingsScope,
    /// Where the settings page's keys are: the rows or the rail.
    focus: SettingsFocus,
    /// The colours' sliders shown when last looked (the log says when that changes).
    colours_shown: Option<[bool; 2]>,
    /// The scroll bar's marker is being dragged.
    bar_dragging: bool,
    /// The chosen row the settings page's window last followed (the wheel and the scroll
    /// bar leave the window where they put it until the choice changes).
    followed: Option<RowId>,
    /// Where the settings page was left this run: what it listed (from the search: what
    /// the search came from), the chosen row and the first line; it opens there again
    /// ([`open_settings`]). Kept while the pause menu is closed.
    last: Option<(SettingsScope, Option<RowId>, usize)>,
    /// The rail's line under the pointer (boxed unfilled, bright).
    rail_hover: Option<RailItem>,
    /// "Report a bug" is under the pointer (boxed unfilled).
    report_hover: bool,
    /// The wheel's movement short of a notch.
    wheel: f32,
    /// A list's open dropdown.
    dropdown: Option<Dropdown>,
    /// The keep question's chosen answer.
    keep_selected: usize,
    /// The load page asks whether to delete the save at this row.
    delete_ask: Option<usize>,
    /// The settings page's search: its text and what it came from.
    search: SearchState,
    /// The rows the search finds and the rows away from their default, as last worked out.
    found: Found,
    /// The Changed list's rows when last logged (the log says when they change).
    changed_logged: Option<Vec<RowId>>,
    /// The settings list's place when last logged ([`log_list_box`]).
    list_logged: Option<ListBox>,
}

impl PauseView {
    /// The settings page's window's lines: as set this frame ([`PauseView::set_lines`]), else
    /// the layout's.
    fn lines(&self) -> usize {
        if self.lines == 0 {
            window_lines()
        } else {
            self.lines
        }
    }

    /// The window's lines from the layout, before the keys and the pointer are read; a new
    /// count is logged, and the first line shown is brought into the new window by
    /// `settings_now` (`settings_view::follow`) [design: #2377].
    fn set_lines(&mut self) {
        let lines = window_lines();
        if lines != self.lines {
            info!("pause menu: settings window of {lines} lines");
            self.lines = lines;
            // A drag of the scroll bar's marker was for the window before.
            self.bar_dragging = false;
        }
    }
}

/// The settings page's search (module notes) [design: #1953].
#[derive(Clone, Debug, Default, PartialEq)]
struct SearchState {
    query: String,
    /// What the page listed when the search opened: its scope, chosen row and first line
    /// (Esc goes back to it); `None`: the search is not open.
    from: Option<(SettingsScope, Option<RowId>, usize)>,
    /// What [`Found::search`] was worked out for: the query, the language's count of
    /// changes and the rows registered.
    key: Option<(String, u32, usize)>,
    /// The query last logged.
    logged: Option<String>,
}

/// What the search took this frame, in the order typed ([`collect_search_keys`]).
#[derive(Clone, Debug, PartialEq, Eq)]
enum SearchKey {
    Text(String),
    Backspace,
    /// Enter: from the text to the results.
    Enter,
    /// Esc: the search closes.
    Escape,
}

/// The search's keys of this frame, gathered in `PreUpdate` before the pause menu reads
/// its keys.
#[derive(Resource, Default, Debug)]
struct SearchKeys(Vec<SearchKey>);

/// What the pause menu aid feeds the next frame's input as real input
/// (`NVF_PLAY_TEST_PAUSE_MENU` `type:`, `backspace`, `ctrl-f`, `point:`, `press`,
/// `release`): keyboard messages, and the pointer's moves and left button.
#[derive(Resource, Default, Debug)]
struct PauseInputAid {
    keys: Vec<KeyboardInput>,
    /// The keys pressed last frame, let go this one.
    up: Vec<KeyboardInput>,
    pointer: Vec<PointerStep>,
}

/// A step of the pointer aid.
#[derive(Clone, Debug, PartialEq)]
enum PointerStep {
    /// Onto the middle of what is drawn there (the aid's word for it, and what it means).
    Point(String, PointAt),
    Press,
    Release,
}

/// What `point:<target>` names: a button by what it is, a tab, a row by its key (or a
/// Controls row's name), a choice of the open dropdown by its place.
#[derive(Clone, Debug, PartialEq)]
enum PointAt {
    Button(PauseButton),
    Tab(String),
    Row(String),
    Choice(usize),
}

/// A list's open dropdown: its row, the choice the keys are on and the first shown.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Dropdown {
    row: usize,
    at: usize,
    first: usize,
}

/// Everything the page drawn shows: drawn again when it changes.
#[derive(Clone, PartialEq, Debug, Default)]
struct Drawn {
    page: Page,
    /// The settings page's lines, what it lists, where its keys are, its rail.
    entries: Vec<ViewEntry>,
    scope: SettingsScope,
    focus: SettingsFocus,
    rail: Vec<RailItem>,
    /// The rail's line under the pointer.
    rail_hover: Option<RailItem>,
    /// The Controls tab's control waiting for its new key or button.
    listening: Option<crate::bindings::Control>,
    main_selected: usize,
    confirm: Option<usize>,
    /// Every row's state (`Setting::value`).
    states: Vec<f32>,
    /// Explanations replacing disabled rows' saved values.
    disabled: Vec<Option<&'static str>>,
    status: String,
    /// The column's labels and whether each can be taken.
    column: Vec<(&'static str, bool)>,
    /// Under the main menu (its entries make the column; no pause screen).
    under_main: bool,
    /// The Mods page shows a mod's own window with its own Back (MCM's): ours is not
    /// drawn over it (`crate::tiles::mods::ModsPage::mcm_open`) [design].
    mod_window: bool,
    /// The load page's thumbnail: the row it is of and whether there is one.
    thumb: Option<(usize, bool)>,
    /// The Controls tab's values, in `GAME_CONTROLS`' order, on the device in use.
    controls: Vec<String>,
    /// The device in use: its name, and whether it is a controller.
    device: (&'static str, bool),
    automatic_input: bool,
    /// The controller rumbles.
    rumble: bool,
    /// The vertical look is inverted.
    invert_y: bool,
    /// The device in use's look sensitivity, a multiple of the game's.
    sensitivity: f32,
    /// The device in use's Vertical Sensitivity, a multiple of its horizontal look.
    vertical: f32,
    /// The settings page's (or the load page's) first row shown ([`PauseView::scroll`]).
    scroll: usize,
    /// The settings page's window's lines ([`PauseView::lines`]).
    lines: usize,
    /// The list rows: each one's row, choices and chosen one.
    lists: Vec<(usize, Vec<String>, String)>,
    /// The list rows that show their choices by other words or hold some out of reach
    /// (`crate::settings::ChoiceLooks`; the others have none here): each one's row and its
    /// words and reasons.
    looks: Vec<(usize, ListLooks)>,
    dropdown: Option<Dropdown>,
    /// The keep question: its row's label, the seconds left and the chosen answer.
    keep: Option<(&'static str, u32, usize)>,
    /// The load page's delete question: the row it asks about.
    delete_ask: Option<usize>,
    /// The view's width in interface units (960 high), rounded.
    view_w: u32,
    /// The language's count of changes (`fnv_util::words::generation`): a new language
    /// draws the page again in it.
    language: u32,
    /// On the search: its text and whether the keys are in it.
    search: Option<(String, bool)>,
    /// On the search and the Changed list: the rows listed as another row, by that row,
    /// their names as the page shows them (`crate::settings_view::stands_for`).
    stand_ins: Vec<(RowId, Vec<String>)>,
    /// "Report a bug" at the bottom left ([`ReportEntry`]), and whether the pointer is on it.
    report: Option<bool>,
}

impl Drawn {
    /// List row `i`'s choices and chosen one.
    fn list(&self, i: usize) -> Option<(&[String], &str)> {
        self.lists.iter().find(|l| l.0 == i).map(|l| (l.1.as_slice(), l.2.as_str()))
    }

    fn looks(&self, i: usize) -> Option<&ListLooks> {
        self.looks.iter().find(|l| l.0 == i).map(|l| &l.1)
    }

    /// The words shown for choice `k` (named `name`) of list row `i`.
    fn choice_words<'a>(&'a self, i: usize, k: usize, name: &'a str) -> &'a str {
        self.looks(i).and_then(|l| l.labels.get(k)).map_or(name, String::as_str)
    }

    /// Why choice `k` of list row `i` cannot be taken now; `None`: it can.
    fn choice_reason(&self, i: usize, k: usize) -> Option<&str> {
        self.looks(i)?.reasons.get(k)?.as_deref()
    }

    /// List row `i`'s chosen one as shown: its words (`chosen`, its name, without any) and
    /// whether it is out of reach now (a stored request it cannot meet).
    fn chosen_words<'a>(&'a self, i: usize, chosen: &'a str) -> (&'a str, bool) {
        self.looks(i).map_or((chosen, false), |l| (l.chosen.0.as_str(), l.chosen.1.is_some()))
    }
}

/// A list row's choices as the page shows them (`crate::settings::ChoiceLooks`), in the
/// order of its names.
#[derive(Clone, PartialEq, Debug, Default)]
struct ListLooks {
    labels: Vec<String>,
    /// Why each cannot be taken now; `None`: it can.
    reasons: Vec<Option<String>>,
    /// The chosen one's words and reason (it may be out of reach: loaded from the INI).
    chosen: (String, Option<String>),
}

impl Drawn {
    /// The column's entry that stands for the page.
    fn page_entry(&self) -> usize {
        let find = |label: &str| self.column.iter().position(|(l, _)| l.starts_with(label)).unwrap_or(0);
        match self.page {
            Page::Main if self.confirm.is_some() => find("Quit"),
            Page::Main => self.main_selected,
            Page::Settings { .. } => find("Settings"),
            Page::Load { .. } => find("Load"),
            Page::Mods => find(MODS_LABEL),
        }
    }
}

/// A list's window: the rows shown at once and each one's height.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Window {
    shown: usize,
    row: f32,
}

/// How many lines the settings page's window shows at once (headings count): as many as fit
/// between the rows' top and a note of [`NOTE_LINES`] lines over [`REPORT_TOP`], at least
/// one; 12 with today's layout [design: #2377]. Worked out here only and carried as a value
/// ([`PauseView::lines`], [`Drawn::lines`]) to everything that draws, scrolls or points at
/// the window, so a later layout can make it depend on the view.
fn window_lines() -> usize {
    let room = REPORT_TOP - OPTIONS_TOP - NOTE_GAP - NOTE_LINES as f32 * NOTE_STEP;
    ((room / OPTION_ROW).floor().max(1.0)) as usize
}

/// The settings page's (and the Controls tab's) window of `lines` rows.
fn settings_window(lines: usize) -> Window {
    Window { shown: lines.max(1), row: OPTION_ROW }
}
/// The load page's window of saves.
const LOAD_WINDOW: Window = Window { shown: LOAD_ROWS, row: ITEM };

impl Window {
    /// The height of the scroll bar's track, between its arrows.
    fn track(self) -> f32 {
        self.shown as f32 * self.row - 2.0 * BAR_ARROW
    }

    /// The scroll bar's marker for a list of `rows` from `first`: its top in the track
    /// and its height, or `None` when all the rows fit. A track shorter than the marker's
    /// least height (a window of a line or two) is filled by the marker, which then stays
    /// at the top.
    fn marker(self, rows: usize, first: usize) -> Option<(f32, f32)> {
        let last = rows.checked_sub(self.shown).filter(|l| *l > 0)?;
        let track = self.track().max(0.0);
        let h = (track * self.shown as f32 / rows as f32).max(BAR_MARKER_MIN.min(track)).min(track);
        let travel = track - h;
        Some((if travel > 0.0 { first.min(last) as f32 / last as f32 * travel } else { 0.0 }, h))
    }

    /// The first row shown with the marker's middle at `y` down the track (0 its top, 1
    /// its bottom); with no room for the marker to move, `y` as a part of the list.
    fn first_at(self, rows: usize, y: f32) -> usize {
        let Some((_, h)) = self.marker(rows, 0) else { return 0 };
        let last = rows - self.shown;
        let track = self.track().max(0.0);
        let travel = track - h;
        let at = if travel > 0.0 { (y * track - h / 2.0) / travel } else { y };
        ((at.clamp(0.0, 1.0) * last as f32).round() as usize).min(last)
    }

    /// Row `i` of a list shown from `first` is in the window (a row the pointer can take).
    fn holds(self, first: usize, i: usize) -> bool {
        (first..first + self.shown).contains(&i)
    }
}

pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        let mut aid = match std::env::var("NVF_PLAY_TEST_PAUSE") {
            Ok(v) => parse_pause_aid(&v).unwrap_or_else(|e| {
                warn!("NVF_PLAY_TEST_PAUSE: {e}");
                PauseAid::default()
            }),
            Err(_) => PauseAid::default(),
        };
        if let Ok(v) = std::env::var("NVF_PLAY_TEST_SETTINGS") {
            match parse_settings_aid(&v) {
                Ok(s) => aid.switches = s,
                Err(e) => warn!("NVF_PLAY_TEST_SETTINGS: {e}"),
            }
        }
        if let Ok(v) = std::env::var("NVF_PLAY_TEST_PAUSE_MENU") {
            match parse_settings_aid(&v) {
                Ok(s) => aid.actions = s,
                Err(e) => warn!("NVF_PLAY_TEST_PAUSE_MENU: {e}"),
            }
        }
        app.init_resource::<Paused>()
            .init_resource::<PageRequest>()
            .init_resource::<Page>()
            .init_resource::<LoadList>()
            .init_resource::<PauseStatus>()
            .init_resource::<PauseView>()
            .init_resource::<Listening>()
            .init_resource::<SettingsLayout>()
            .init_resource::<SearchKeys>()
            .init_resource::<SettingsTabStore>()
            .add_systems(Startup, read_settings_tab)
            .init_resource::<PauseInputAid>()
            .init_resource::<crate::bindings::KeyboardCapture>()
            .init_resource::<FormOver>()
            .init_resource::<ReportEntry>()
            .init_resource::<ModsEntry>()
            .init_resource::<crate::tiles::mods::ModsPage>()
            .add_message::<MainMenuRequest>()
            .init_resource::<MenuPause>()
            .init_resource::<PlayClock>()
            .init_resource::<crate::graphics_resume::GraphicsResume>()
            .insert_resource(aid)
            .add_systems(Update, tick_play_clock.before(PlaySet::Input))
            // The aid's keys go in as the window's would, before Bevy reads them; the search's
            // text is taken after (and after the controller's menu keys), before the game's
            // input reads the keys; the aid's pointer before the UI looks under it.
            .add_systems(PreUpdate, type_aid.before(bevy::input::InputSystems))
            .add_systems(PreUpdate, collect_search_keys.after(bevy::input::InputSystems).after(crate::pad::menu_keys))
            .add_systems(PreUpdate, pointer_aid.after(bevy::input::InputSystems).before(bevy::ui::UiSystems::Focus))
            .add_systems(Update, freeze_picture.after(PlaySet::Ui).before(PlaySet::Sync))
            .add_systems(
                Update,
                // Esc pauses a conversation too; resuming goes back to it (`crate::dialogue`).
                // Over a conversation, a barter menu takes Esc for itself (`crate::menu`).
                (
                    open.run_if(in_state(PlayState::Playing).or_else(in_state(PlayState::Dialogue)).and_then(crate::menu::no_menu)),
                    (choose.run_if(no_form), finish_resume, show_status, draw).chain().run_if(in_state(PlayState::Menu)),
                )
                    .in_set(PlaySet::Ui),
            )
            .add_systems(OnExit(PlayState::Menu), close);
        add_setting(
            app,
            Setting {
                key: "menu-pause",
                group: GROUP_MENUS,
                label: "Freeze the picture in dialogue and menus",
                note: "On: animations, wind, water and sky stand still in conversations, the inventory and other menus, as in the original. Off: only game logic waits.",
                switches: "--menu-pause",
                get: |w| *w.resource::<MenuPause>() == MenuPause::Original,
                set: |w, on| *w.resource_mut::<MenuPause>() = if on { MenuPause::Original } else { MenuPause::Living },
                slider: None,
            },
        );
    }
}


fn full() -> Node {
    Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }
}

/// A node `width` wide whose right edge is `right` from the view's.
fn from_right(right: f32, top: f32, width: f32, height: f32) -> Node {
    Node { position_type: PositionType::Absolute, right: u(right), top: u(top), width: u(width), height: u(height), ..default() }
}

/// A node whose left edge is `left` from the view's middle.
fn from_middle(left: f32, top: f32, width: f32, height: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: Val::Percent(50.0), margin: UiRect::left(u(left)), top: u(top), width: u(width), height: u(height), ..default() }
}

/// Back at the bottom right with its hint, as Delete has its own: "Back  Esc)", with a
/// controller "Back  B)" [observed, issue #1893: the original's Settings pages show
/// "Back" with the controller's B; the keyboard's key in the same place: design].
/// Defaults keeps no hint: the original's is Y, which here puts the keys on the rail
/// (#1953), not a press of Defaults.
fn back_label() -> String {
    back_label_as(crate::pad::labels())
}

/// [`back_label`] with the labels `l`.
fn back_label_as(l: crate::pad::Labels) -> String {
    format!("{}  {}", tr("Back"), crate::pad::hint_as(&[KeyCode::Escape], crate::pad::PAUSE_AT, l))
}

/// A right-aligned list entry: the whole row takes the pointer, the box round it when
/// `boxed`.
#[allow(clippy::too_many_arguments)]
fn right_entry(p: &mut ChildSpawnerCommands, face: &Face, label: &str, node: Node, width: f32, shade: Shade, boxed_on: bool, b: PauseButton, glyphs: &crate::pad::PadGlyphs) {
    let look = if boxed_on { BoxLook::Filled } else { BoxLook::None };
    right_entry_look(p, face, label, node, width, shade, look, b, glyphs);
}

/// How an entry is boxed: not, the highlight box filled (the keys' choice), or its line
/// alone (under the pointer) [design: #1953; as the screen menu's page names,
/// docs/design/screen-menu.md §3.1].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoxLook {
    None,
    Filled,
    Outline,
}

/// [`right_entry`] with its box's look.
#[allow(clippy::too_many_arguments)]
fn right_entry_look(p: &mut ChildSpawnerCommands, face: &Face, label: &str, node: Node, width: f32, shade: Shade, look: BoxLook, b: PauseButton, glyphs: &crate::pad::PadGlyphs) {
    let w = crate::pad::hint_width(face, label, glyphs);
    let (node, line, fill) = boxed(node, look != BoxLook::None);
    let fill = if look == BoxLook::Outline { BackgroundColor(Color::NONE) } else { fill };
    p.spawn((Button, b, node, line, fill)).with_children(|t| {
        crate::pad::set_hint(t, face, label, width - w - 10.0, BASE, color(shade), glyphs);
    });
}

/// A node with the highlight box round it when `on`.
fn boxed(node: Node, on: bool) -> (Node, BorderColor, BackgroundColor) {
    let (line, fill) = highlight_colors();
    if on {
        (Node { border: UiRect::all(u(LINE)), ..node }, BorderColor::all(line), BackgroundColor(fill))
    } else {
        (node, BorderColor::all(Color::NONE), BackgroundColor(Color::NONE))
    }
}

/// The darkened band behind a page's list.
fn band(root: &mut ChildSpawnerCommands, look: &PauseLook, node: Node) {
    // Under it a sheet of black, so the list reads over a bright main menu screen too.
    root.spawn((node.clone(), BackgroundColor(Color::BLACK.with_alpha(BAND_SHEET))));
    match &look.band {
        Some(b) => {
            root.spawn((node, ImageNode { color: Color::WHITE.with_alpha(BAND_ALPHA), image_mode: NodeImageMode::Stretch, ..ImageNode::new(b.clone()) }));
        }
        None => {
            root.spawn((node, BackgroundColor(Color::BLACK.with_alpha(BAND_ALPHA * 0.6))));
        }
    }
}

/// Draw the page `d` (the one before it is gone already).
fn spawn_page(commands: &mut Commands, d: &Drawn, look: &PauseLook, settings: &[Setting], list: &LoadList, glyphs: &crate::pad::PadGlyphs) {
    if d.under_main && d.page == Page::Main {
        return;
    }
    let f = &look.faces;
    // The settings list's place, clear of the categories.
    let lb = ListBox::of(d.view_w as f32, rail_width(f, &d.rail, d.device.1));
    commands.spawn((PauseRoot, full(), GlobalZIndex(110))).with_children(|root| {
        // The pause screen over the held world.
        if !d.under_main {
            root.spawn((full(), BackgroundColor(Color::BLACK.with_alpha(WORLD_SHADE))));
            if let Some(s) = &look.screen {
                // Each half drawn larger than the view's half, its cropped parts off the view.
                let width = 50.0 / (1.0 - SCREEN_CROP_SIDE);
                let height = 100.0 / (1.0 - SCREEN_CROP_TOP - SCREEN_CROP_BOTTOM);
                let half = |left: bool| Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(if left { 50.0 - width } else { 50.0 }),
                    top: Val::Percent(-SCREEN_CROP_TOP * height),
                    width: Val::Percent(width),
                    height: Val::Percent(height),
                    ..default()
                };
                let img = |flip: bool| ImageNode { color: Color::WHITE.with_alpha(SCREEN_ALPHA), flip_x: flip, image_mode: NodeImageMode::Stretch, ..ImageNode::new(s.clone()) };
                root.spawn((half(true), img(false)));
                root.spawn((half(false), img(true)));
            } else {
                root.spawn((full(), BackgroundColor(Color::srgba(0.30, 0.24, 0.14, SCREEN_ALPHA))));
            }
        }
        // The page at the UI scale, in the middle of the view (`crate::interface::menu_frame`,
        // #1631); the sheet and the screen above keep to the view.
        crate::interface::in_menu_frame(root, crate::interface::MIDDLE, |root| {
            // The band behind a page's list, from the view's top to its bottom.
            let tall = |n: Node| crate::interface::over_view(n, crate::interface::MIDDLE, false);
            match d.page {
                Page::Settings { .. } => {
                    let (band_left, band_w) = lb.band();
                    band(root, look, tall(Node { top: u(0.0), height: Val::Percent(100.0), ..from_middle(band_left, 0.0, band_w, 0.0) }))
                }
                Page::Load { .. } => band(root, look, tall(Node { height: Val::Percent(100.0), ..from_right(RIGHT + COLUMN, 0.0, LOAD_WIDTH + BAND_MARGIN, 0.0) })),
                // The mod's page is drawn from its tiles over this one (`crate::tiles::draw`).
                Page::Main | Page::Mods => {}
            }
            // The column: the keys' list on the first page, else half bright but for the page's entry.
            let top = (960.0 - d.column.len() as f32 * ITEM) / 2.0;
            let on_column = d.page == Page::Main && d.confirm.is_none();
            let lit = d.page_entry();
            for (i, (label, ok)) in d.column.iter().enumerate() {
                let shade = if (on_column && *ok) || (!on_column && i == lit) { Shade::Bright } else { Shade::Dim };
                right_entry(root, &f.entry, tr_str(label), from_right(RIGHT, top + i as f32 * ITEM, COLUMN - 5.0, ITEM), COLUMN - 5.0, shade, on_column && i == d.main_selected, PauseButton::Column(i), glyphs);
            }
            match d.page {
                Page::Main => {
                    if let Some(sel) = d.confirm {
                        // The question: its lines end left of the choices [observed: the owner's screenshot].
                        let (warning, question) = (tr(WARNING), tr(QUESTION));
                        let qw = f.entry.font.width(question);
                        let at = 280.0;
                        root.spawn(from_middle(at - 20.0 - qw, 480.0 - ITEM, 900.0, ITEM)).with_children(|t| sm_text(t, &f.entry, warning, 0.0, BASE, color(Shade::Bright)));
                        root.spawn(from_middle(at - 20.0 - qw, 480.0, qw, ITEM)).with_children(|t| sm_text(t, &f.entry, question, 0.0, BASE, color(Shade::Bright)));
                        let width = CONFIRM.iter().map(|c| f.entry.font.width(tr(c))).fold(0.0, f32::max) + 30.0;
                        for (i, c) in CONFIRM.iter().enumerate() {
                            root.spawn((Button, PauseButton::Confirm(i), boxed(from_middle(at, 480.0 + i as f32 * ITEM, width, ITEM), i == sel)))
                            .with_children(|t| sm_text(t, &f.entry, tr(c), 10.0, BASE, color(Shade::Bright)));
                        }
                    } else if !d.status.is_empty() {
                        root.spawn((PauseStatusText, from_right(RIGHT, top + d.column.len() as f32 * ITEM + 10.0, 700.0, ITEM))).with_children(|t| {
                            let w = f.small.font.width(&d.status);
                            sm_text(t, &f.small, &d.status, 700.0 - w - 10.0, 30.0, color(Shade::Bright));
                        });
                    }
                }
                Page::Settings { selected } => settings_page(root, d, look, settings, selected, lb, glyphs),
                Page::Load { selected } => load_page(root, d, look, list, selected, glyphs),
                Page::Mods => {}
            }
            // Back (and Defaults) at the bottom right.
            if d.page != Page::Main && !d.mod_window {
                let on = |item| matches!(d.page, Page::Settings { .. }) && d.focus == SettingsFocus::Rail(item);
                right_entry(root, &f.entry, &back_label(), from_right(RIGHT, BACK_TOP, 200.0, ITEM), 200.0, Shade::Bright, on(RailItem::Back), PauseButton::Back, glyphs);
                if matches!(d.page, Page::Settings { .. }) {
                    right_entry(root, &f.entry, tr("Defaults"), from_right(RIGHT, BACK_TOP - ITEM, 200.0, ITEM), 200.0, Shade::Bright, on(RailItem::Defaults), PauseButton::Defaults, glyphs);
                }
            }
            // "Report a bug" over the version at the bottom left; the first page shows the
            // version too (the settings page draws its own).
            if let Some(hovered) = d.report {
                report_entry(root, f, hovered, glyphs);
                if d.page == Page::Main {
                    version(root, f);
                }
            }
            if let Some((label, left, sel)) = d.keep {
                keep_question(root, f, label, left, sel);
            }
        });
    });
}

/// The keep question over the page: whether to keep the choice of row `label`, the
/// seconds left, Keep and Revert.
fn keep_question(root: &mut ChildSpawnerCommands, f: &Faces, label: &str, left: u32, sel: usize) {
    // A sheet over the page that takes the pointer, so only the answers can be clicked.
    root.spawn((crate::interface::over_view(full(), crate::interface::MIDDLE, true), BackgroundColor(Color::BLACK.with_alpha(0.6)), Button, GlobalZIndex(112)));
    let (w, h) = (640.0, 4.0 * ITEM + 40.0);
    let (line, _) = highlight_colors();
    // The row's name as the language writes it; English's in lower case ("Keep this resolution?").
    let row = tr_str(label);
    let row = if row == label { label.to_lowercase() } else { row.to_string() };
    let question = fnv_util::words::trf("Keep this {row}?", &[("row", &row)]);
    let count = fnv_util::words::trf("Going back in {left} s.", &[("left", &left)]);
    root.spawn((KeepLine, Text::new(format!("{question} {count}")), Node { display: Display::None, ..default() }));
    root.spawn((Node { border: UiRect::all(u(LINE)), ..from_middle(-w / 2.0, 480.0 - h / 2.0, w, h) }, BorderColor::all(line), BackgroundColor(Color::BLACK), GlobalZIndex(113))).with_children(|b| {
        for (k, t) in [&question, &count].into_iter().enumerate() {
            let tw = f.entry.font.width(t);
            b.spawn(at(0.0, 20.0 + k as f32 * ITEM, w, ITEM)).with_children(|r| sm_text(r, &f.entry, t, (w - tw) / 2.0, BASE, color(Shade::Bright)));
        }
        let bw = 180.0;
        for (k, a) in KEEP_ANSWERS.iter().enumerate() {
            let x = w / 2.0 + if k == 0 { -bw - 20.0 } else { 20.0 };
            let a = tr(a);
            let aw = f.entry.font.width(a);
            b.spawn((Button, PauseButton::Keep(k), boxed(at(x, 20.0 + 2.0 * ITEM + 20.0, bw, ITEM), k == sel))).with_children(|r| sm_text(r, &f.entry, a, (bw - aw) / 2.0, BASE, color(Shade::Bright)));
        }
    });
}

/// The keep question's lines, kept as text too (tests read it).
#[derive(Component)]
struct KeepLine;

/// The settings page (module notes): the rail left of the column, the heading links over
/// the rows, the listed lines (headings and rows) in a window of [`Drawn::lines`] round the
/// chosen row, the chosen row's note under them, the version at the bottom left.
fn settings_page(root: &mut ChildSpawnerCommands, d: &Drawn, look: &PauseLook, list: &[Setting], selected: Option<RowId>, lb: ListBox, glyphs: &crate::pad::PadGlyphs) {
    let f = &look.faces;
    rail_column(root, f, d, glyphs);
    let top = OPTIONS_TOP;
    let left = lb.left;
    let entries = &d.entries;
    if entries.is_empty() {
        root.spawn(from_middle(left, top, lb.width, OPTION_ROW)).with_children(|t| sm_text(t, &f.entry, tr(empty_words(d.scope)), 10.0, BASE, color(Shade::Dim)));
    }
    // On the search its line takes the links' place, the rows' whole width; Page Up/Down
    // (LB/RB) still go from heading to heading there [design: #1953].
    let links = if d.search.is_none() && entries.iter().any(|e| matches!(e, ViewEntry::Heading(_))) { heading_links(root, f, lb, glyphs) } else { left + lb.width };
    if let Some((query, typing)) = &d.search {
        search_line(root, f, query, *typing, left, links - 10.0 - left);
    }
    let crumbs = !matches!(d.scope, SettingsScope::Tab(_));
    let lines = d.lines.max(1);
    let first = d.scroll.min(entries.len().saturating_sub(lines));
    let drawn = crate::settings_view::drawn_slots(entries, first, lines);
    scroll_bar(root, look, entries.len(), first, top, lb, lines);
    for (n, k) in drawn.clone().enumerate() {
        let y = top + n as f32 * OPTION_ROW;
        match entries[k] {
            ViewEntry::Heading(p) => heading_line(root, f, &heading_words(p, crumbs), lb, y),
            ViewEntry::Row(id) => row_line(root, d, look, list, id, selected == Some(id), lb, y, glyphs),
        }
    }
    // The chosen row's note under the rows; with its list open on a choice out of reach,
    // that choice's reason.
    let reason = d.dropdown.filter(|o| selected == Some(RowId::Setting(o.row))).and_then(|o| d.choice_reason(o.row, o.at));
    let note = match reason {
        Some(r) => tr_str(r).to_string(),
        None => selected.map_or_else(String::new, |id| note_of(list, id, d)),
    };
    root.spawn((SwitchNote, Text::new(note.clone()), Node { display: Display::None, ..default() }));
    let shown = (entries.len() - first.min(entries.len())).min(lines) as f32;
    note_lines(root, f, &note, lb, top + shown * OPTION_ROW + NOTE_GAP);
    version(root, f);
    // The open dropdown over the rows under its own (over those above near the bottom).
    let Some((o, names)) = d.dropdown.and_then(|o| Some((o, d.list(o.row)?.0))) else { return };
    let Some(n) = drawn.clone().position(|k| entries[k] == ViewEntry::Row(RowId::Setting(o.row))) else { return };
    let shown = names.len().min(DROP_ROWS);
    let h = shown as f32 * DROP_ITEM + 2.0 * LINE;
    let row_top = top + n as f32 * OPTION_ROW;
    let y = if row_top + OPTION_ROW + h > 950.0 { row_top - h } else { row_top + OPTION_ROW };
    let (line, _) = highlight_colors();
    root.spawn((Node { border: UiRect::all(u(LINE)), ..from_middle(left + lb.row().list_left(), y, LIST_WIDTH, h) }, BorderColor::all(line), BackgroundColor(Color::BLACK), GlobalZIndex(111))).with_children(|b| {
        for (k, name) in names.iter().enumerate().skip(o.first).take(shown) {
            let name = fit_text(|t| f.entry.font.width(t), tr_str(d.choice_words(o.row, k, name)), LIST_WIDTH - 2.0 * LINE - 2.0 * VALUE_PAD);
            let name = name.as_str();
            let shade = if d.choice_reason(o.row, k).is_some() { Shade::Dim } else { Shade::Bright };
            let nw = f.entry.font.width(name);
            b.spawn((Button, PauseButton::Choice(o.row, k), boxed(at(0.0, (k - o.first) as f32 * DROP_ITEM, LIST_WIDTH - 2.0 * LINE, DROP_ITEM), k == o.at)))
            .with_children(|r| sm_text(r, &f.entry, name, (LIST_WIDTH - nw) / 2.0 - LINE, DROP_ITEM * 0.72, color(shade)));
        }
    });
}

/// A heading's line: its words dim in the page's font, nothing to choose [design: #1953].
fn heading_line(root: &mut ChildSpawnerCommands, f: &Faces, words: &str, lb: ListBox, y: f32) {
    root.spawn((HeadingLine, Text::new(words), Node { display: Display::None, ..default() }));
    let words = fit_text(|t| f.entry.font.width(t), words, lb.width - 20.0);
    root.spawn(from_middle(lb.left, y, lb.width, OPTION_ROW)).with_children(|t| sm_text(t, &f.entry, &words, 10.0, BASE + 2.0, color(Shade::Dim)));
}

/// A heading's words, translated: on a tab its heading's (`Saving`); on the search and the
/// Changed list, which list rows of every tab, where its rows live (`Gameplay > Saving`,
/// `Controls`) [design: #1953].
fn heading_words(p: crate::settings_layout::Placement, crumbs: bool) -> String {
    if !crumbs {
        return tr(p.heading.unwrap_or(p.tab)).to_string();
    }
    [Some(p.tab), p.subgroup, p.heading].into_iter().flatten().map(tr).collect::<Vec<_>>().join(" > ")
}

/// What a list with no rows says: a tab, the search, the Changed list.
fn empty_words(scope: SettingsScope) -> &'static str {
    match scope {
        SettingsScope::Tab(_) => NO_ROWS,
        SettingsScope::Search => NO_RESULTS,
        SettingsScope::Changed => NO_CHANGED,
    }
}

/// The search's line over its results, from the rows' left edge, `width` wide: "Search:"
/// and its text (its start left out when it does not fit), the mark after it and the box
/// round it while the keys are in it; a click puts them there [design: #1953].
fn search_line(root: &mut ChildSpawnerCommands, f: &Faces, query: &str, typing: bool, left: f32, width: f32) {
    let width = width.max(160.0);
    let caret = if typing { CARET } else { "" };
    let line = |q: &str| format!("{}: {q}{caret}", tr(SEARCH));
    let mut shown = query.to_string();
    while !shown.is_empty() && f.entry.font.width(&line(&shown)) > width - 20.0 {
        shown.remove(0);
    }
    let words = line(&shown);
    root.spawn((SearchText, Text::new(words.clone()), Node { display: Display::None, ..default() }));
    root.spawn((Button, PauseButton::SearchField, boxed(from_middle(left, LINKS_TOP, width, ITEM), typing))).with_children(|t| sm_text(t, &f.entry, &words, 10.0, BASE, color(Shade::Bright)));
}

/// The search's line, kept as text too (tests read it).
#[derive(Component)]
struct SearchText;

/// "Previous heading" and "Next heading" over the rows' right end, each with its key; the
/// left edge they leave.
fn heading_links(root: &mut ChildSpawnerCommands, f: &Faces, lb: ListBox, glyphs: &crate::pad::PadGlyphs) -> f32 {
    let mut right = lb.left + lb.width;
    for (k, key) in [(1, KeyCode::PageDown), (0, KeyCode::PageUp)] {
        let label = format!("{}  {}", tr(HEADING_LINKS[k]), crate::pad::hint(&[key], crate::pad::PAUSE_AT));
        let w = crate::pad::hint_width(&f.small, &label, glyphs) + 20.0;
        right -= w;
        right_entry(root, &f.small, &label, from_middle(right, LINKS_TOP, w, ITEM), w, Shade::Bright, false, PauseButton::HeadingStep(if k == 0 { -1 } else { 1 }), glyphs);
        right -= 10.0;
    }
    right
}

/// A row's line: its label, its value (ON or OFF, a meter, a list's box, a key), the box
/// round it when chosen. The one drawing of a row wherever it is listed.
#[allow(clippy::too_many_arguments)]
fn row_line(root: &mut ChildSpawnerCommands, d: &Drawn, look: &PauseLook, list: &[Setting], id: RowId, on: bool, lb: ListBox, y: f32, glyphs: &crate::pad::PadGlyphs) {
    let f = &look.faces;
    let x = lb.row();
    let width = |t: &str| f.entry.font.width(t);
    let node = boxed(from_middle(lb.left, y, lb.width, OPTION_ROW), on);
    match id {
        RowId::Setting(i) => {
            let Some(s) = list.get(i) else { return };
            let v = d.states.get(i).copied().unwrap_or(0.0);
            let disabled = d.disabled.get(i).copied().flatten();
            let label = tr(s.label);
            let label_shade = color(if disabled.is_some() { Shade::Dim } else { Shade::Bright });
            root.spawn((Button, PauseButton::Row(id), node)).with_children(|r| {
                if let Some(reason) = disabled {
                    let reason = tr(reason);
                    r.spawn((SwitchState(id), Text::new(reason), Node { display: Display::None, ..default() }));
                    let parts = row_parts(x, ValueKind::Centred(width(reason)));
                    let (label, reason) = fit_row(width, &parts, label, reason);
                    sm_text(r, &f.entry, &label, parts.label.0, BASE + 2.0, label_shade);
                    sm_text(r, &f.entry, &reason, parts.text_x(width(&reason)), BASE + 2.0, color(Shade::Dim));
                    return;
                }
                if let Some(slider) = &s.slider {
                    let value = slider.text(v);
                    let parts = row_parts(x, ValueKind::Slider(width(tr_str(&value))));
                    sm_text(r, &f.entry, &fit_text(width, label, parts.label_room()), parts.label.0, BASE + 2.0, label_shade);
                    slider_row(r, look, &parts, id, v, &value);
                    return;
                }
                if let Some((_, chosen)) = d.list(i) {
                    let shown = d.chosen_words(i, chosen);
                    let parts = row_parts(x, ValueKind::List(width(tr_str(shown.0))));
                    sm_text(r, &f.entry, &fit_text(width, label, parts.label_room()), parts.label.0, BASE + 2.0, label_shade);
                    list_row(r, look, &parts, id, chosen, shown, d.dropdown.is_some_and(|o| o.row == i));
                    return;
                }
                let value = tr(if v > 0.5 { "ON" } else { "OFF" });
                r.spawn((SwitchState(id), Text::new(value), Node { display: Display::None, ..default() }));
                let parts = row_parts(x, ValueKind::Centred(width(value)));
                let (label, value) = fit_row(width, &parts, label, value);
                sm_text(r, &f.entry, &label, parts.label.0, BASE + 2.0, label_shade);
                sm_text(r, &f.entry, &value, parts.text_x(width(&value)), BASE + 2.0, color(Shade::Bright));
            });
        }
        RowId::Control(c) => {
            let (name, value) = control_line(d, c);
            root.spawn((Button, PauseButton::Row(id), node)).with_children(|r| {
                if let ControlRow::Sensitivity(_) | ControlRow::Vertical(_) = c {
                    let m = if matches!(c, ControlRow::Vertical(_)) { d.vertical } else { d.sensitivity };
                    let parts = row_parts(x, ValueKind::Slider(width(tr_str(&value))));
                    sm_text(r, &f.entry, &fit_text(width, name, parts.label_room()), parts.label.0, BASE + 2.0, color(Shade::Bright));
                    slider_row(r, look, &parts, id, crate::bindings::sensitivity_part(m), &value);
                    return;
                }
                let vw = crate::pad::button_width(&f.entry, &value, glyphs);
                let parts = row_parts(x, ValueKind::Binding(vw));
                sm_text(r, &f.entry, &fit_text(width, name, parts.label_room()), parts.label.0, BASE + 2.0, color(Shade::Bright));
                // The full words stay in the state the aids read; what is drawn is cut to the
                // row (a button's picture is narrow and always fits).
                r.spawn((SwitchState(id), Text::new(value.clone()), Node { display: Display::None, ..default() }));
                let (shown, sw) = if vw > parts.text_room() + 0.01 {
                    let cut = fit_text(width, &value, parts.text_room());
                    let w = width(&cut);
                    (cut, w)
                } else {
                    (value.clone(), vw)
                };
                crate::pad::set_button(r, &f.entry, &shown, parts.text_x(sw), BASE + 2.0, color(Shade::Bright), glyphs);
            });
        }
    }
}

/// A Controls row's label and value, on the device in use.
fn control_line(d: &Drawn, c: ControlRow) -> (&'static str, String) {
    let (device, controller) = d.device;
    let on_off = |on: bool| tr(if on { "ON" } else { "OFF" }).to_string();
    match c {
        ControlRow::Device => (DEVICE_ROW, device.to_string()),
        ControlRow::AutomaticInput => (AUTOMATIC_INPUT_ROW, on_off(d.automatic_input)),
        ControlRow::Binding(k, _) if d.listening == Some(k) => (crate::bindings::label(k), (if controller { WAITING_PAD } else { WAITING }).to_string()),
        ControlRow::Binding(k, _) => {
            let at = crate::bindings::GAME_CONTROLS.iter().position(|(x, _, _)| *x == k);
            (crate::bindings::label(k), at.and_then(|i| d.controls.get(i)).cloned().unwrap_or_default())
        }
        ControlRow::Invert => (INVERT_ROW, on_off(d.invert_y)),
        ControlRow::Sensitivity(_) => (SENSITIVITY_ROWS[usize::from(controller)], crate::bindings::sensitivity_text(d.sensitivity)),
        ControlRow::Vertical(_) => (VERTICAL_ROW, crate::bindings::sensitivity_text(d.vertical)),
        ControlRow::Rumble => (RUMBLE_ROW, on_off(d.rumble)),
    }
}

/// The note under the rows for row `id`.
fn row_note(list: &[Setting], id: RowId, d: &Drawn) -> String {
    use crate::bindings::label;
    let controller = d.device.1;
    let c = match id {
        RowId::Setting(i) => return list.get(i).map_or_else(String::new, |s| tr(s.note).to_string()),
        RowId::Control(c) => c,
    };
    match c {
        ControlRow::AutomaticInput => tr(AUTOMATIC_INPUT_NOTE).to_string(),
        ControlRow::Invert => format!("Enter or a click: look up and down the other way round (the mouse and the right stick). Kept as \"{INVERT_ROW}\" in [Controls]."),
        ControlRow::Sensitivity(_) if controller => format!(
            "Left/Right or a click on the meter: how fast the right stick turns the view, {} to {} of the game's. Kept as \"Stick Sensitivity\" in [Controller].",
            crate::bindings::sensitivity_text(crate::bindings::SENSITIVITY_LEAST),
            crate::bindings::sensitivity_text(crate::bindings::SENSITIVITY_MOST)
        ),
        ControlRow::Sensitivity(_) => format!(
            "Left/Right or a click on the meter: how far the view turns as the mouse moves, {} to {} of the game's. Kept as \"Mouse Sensitivity\" in [Controls].",
            crate::bindings::sensitivity_text(crate::bindings::SENSITIVITY_LEAST),
            crate::bindings::sensitivity_text(crate::bindings::SENSITIVITY_MOST)
        ),
        ControlRow::Vertical(_) if controller => format!(
            "Left/Right or a click on the meter: how fast the right stick turns the view up and down, as a part of left and right ({} to {}).",
            crate::bindings::sensitivity_text(crate::bindings::SENSITIVITY_LEAST),
            crate::bindings::sensitivity_text(crate::bindings::SENSITIVITY_MOST)
        ),
        ControlRow::Vertical(_) => format!(
            "Left/Right or a click on the meter: how far the view turns up and down as the mouse moves, as a part of left and right ({} to {}).",
            crate::bindings::sensitivity_text(crate::bindings::SENSITIVITY_LEAST),
            crate::bindings::sensitivity_text(crate::bindings::SENSITIVITY_MOST)
        ),
        ControlRow::Rumble => format!("A or a click: the controller shakes when you fire, take damage, land a hard fall or an explosion goes off near you. Kept as \"{RUMBLE_ROW}\" in [Controller]."),
        ControlRow::Device => format!("Enter or a click: work the controls with the keyboard and mouse or with a controller. Kept as \"{DEVICE_ROW}\" in [Controls] of NVForever.ini."),
        ControlRow::Binding(c, _) if d.listening == Some(c) && controller => format!("Press the new controller button for {}. Start cancels.", label(c)),
        ControlRow::Binding(c, _) if d.listening == Some(c) => format!("Press the new key or mouse button for {}. Esc cancels.", label(c)),
        ControlRow::Binding(c, _) if controller => format!("A or a click: bind a new controller button to {} (a control that had it gets the old one). X or Delete clears it.", label(c)),
        ControlRow::Binding(c, _) => format!("Enter or a click: bind a new key or mouse button to {} (a control that had it gets the old one). Delete clears its key.", label(c)),
    }
}

/// A list's row: its choice in a box with a down arrow (lit while its dropdown is open),
/// shown by its words (`shown`, cut to the box where they do not fit), dim when out of reach
/// now.
fn list_row(r: &mut ChildSpawnerCommands, look: &PauseLook, parts: &RowParts, i: RowId, chosen: &str, shown: (&str, bool), open: bool) {
    let bright = color(Shade::Bright);
    let (line, fill) = highlight_colors();
    let list_left = parts.value.0;
    r.spawn((Node { border: UiRect::all(u(LINE)), ..at(list_left, 6.0, LIST_WIDTH, OPTION_ROW - 12.0) }, BorderColor::all(line.with_alpha(if open { 1.0 } else { 0.6 })), BackgroundColor(if open { fill } else { Color::NONE })));
    // The state kept as text stays the English name (tests and aids read it); the page
    // shows it in the language chosen.
    r.spawn((SwitchState(i), Text::new(chosen), Node { display: Display::None, ..default() }));
    let (words, out_of_reach) = shown;
    let font = &look.faces.entry.font;
    let words = fit_text(|t| font.width(t), tr_str(words), parts.text_room());
    sm_text(r, &look.faces.entry, &words, parts.text_x(font.width(&words)), BASE + 2.0, if out_of_reach { color(Shade::Dim) } else { bright });
    // The right arrow turned to point down.
    let node = at(list_left + LIST_WIDTH - 30.0, (OPTION_ROW - METER_ARROW.y) / 2.0, METER_ARROW.x, METER_ARROW.y);
    let turned = UiTransform { rotation: Rot2::degrees(90.0), ..default() };
    match &look.arrows[1] {
        Some(image) => r.spawn((node, turned, ImageNode { color: bright, ..ImageNode::new(image.clone()) })),
        None => r.spawn((node, turned, BackgroundColor(bright.with_alpha(0.5)))),
    };
}

/// The rail left of the column, from the rows' top: Search, Changed and the tabs, what the
/// page lists bright, the others dim, the box round the line the keys are on (F6)
/// [design: #1953]. Defaults and Back keep their place at the bottom right.
fn rail_column(root: &mut ChildSpawnerCommands, f: &Faces, d: &Drawn, glyphs: &crate::pad::PadGlyphs) {
    let lines = d.rail.iter().filter(|r| !matches!(r, RailItem::Defaults | RailItem::Back));
    for (i, item) in lines.enumerate() {
        let name = rail_label(*item, d.device.1);
        let w = SUB_WIDTH.max(f.entry.font.width(name) + 20.0);
        // The line shown and the line under the pointer bright; the keys' line boxed
        // filled, the pointer's boxed unfilled [design: #1953].
        let hovered = d.rail_hover == Some(*item);
        let shade = if RailItem::of(d.scope) == *item || hovered { Shade::Bright } else { Shade::Dim };
        let look = if d.focus == SettingsFocus::Rail(*item) {
            BoxLook::Filled
        } else if hovered {
            BoxLook::Outline
        } else {
            BoxLook::None
        };
        right_entry_look(root, &f.entry, name, from_right(SUB_RIGHT, OPTIONS_TOP + i as f32 * ITEM, w, ITEM), w, shade, look, PauseButton::Rail(*item), glyphs);
    }
}

/// The note under the settings page's rows from `top`, wrapped to the rows' width and cut
/// to the lines that are drawn ([`note_fit`]).
fn note_lines(root: &mut ChildSpawnerCommands, f: &Faces, note: &str, lb: ListBox, top: f32) {
    let (left, width) = (lb.left, note_width(lb));
    let lines = note_wrapped(&f.small.font, note, width);
    let fit = note_fit(lines.len(), top);
    let bright = color(Shade::Bright);
    root.spawn(from_middle(left + 10.0, top, width, fit as f32 * NOTE_STEP)).with_children(|t| {
        for (i, line) in lines.iter().take(fit).enumerate() {
            crate::interface::set_line(t, &f.small, line, 0.0, NOTE_STEP * (i as f32 + 0.8), bright);
        }
    });
}

/// A note wrapped at `width` in `font`: a line still wider (a word longer than the line,
/// which wrapping leaves whole) cut with [`CUT_MARK`] to the width, so it never runs past
/// the list [design: #2377]. The full note stays in the hidden text the aids read.
fn note_wrapped(font: &fnv_font::Font, note: &str, width: f32) -> Vec<fnv_font::Line> {
    font.lines(note, Some(width))
        .into_iter()
        .map(|line| {
            if line.width <= width {
                return line;
            }
            let cut = fit_text(|t| font.width(t), &line.text(), width);
            font.lines(&cut, None).into_iter().next().unwrap_or_default()
        })
        .collect()
}

/// The note's width in a list `lb`: the rows' less 10 at each side.
fn note_width(lb: ListBox) -> f32 {
    lb.width - 20.0
}

/// How many of a note's `lines` are drawn from `top`: at most [`NOTE_LINES`], and only those
/// above [`NOTE_BOTTOM`].
fn note_fit(lines: usize, top: f32) -> usize {
    lines.min(NOTE_LINES).min(((NOTE_BOTTOM - top) / NOTE_STEP).max(0.0) as usize)
}

/// The rail's width: its widest line's, translated ([`rail_column`]).
fn rail_width(f: &Faces, rail: &[RailItem], controller: bool) -> f32 {
    rail.iter().filter(|r| !matches!(r, RailItem::Defaults | RailItem::Back)).map(|r| SUB_WIDTH.max(f.entry.font.width(rail_label(*r, controller)) + 20.0)).fold(SUB_WIDTH, f32::max)
}

/// A rail line's words, translated: Search says "Keyboard search" while a controller works
/// the controls (its keys pass over it) [design: #1953].
fn rail_label(item: RailItem, controller: bool) -> &'static str {
    match item {
        RailItem::Search if controller => tr(KEYBOARD_SEARCH),
        _ => tr(item.name()),
    }
}

/// The settings list's place in the view: its left edge from the view's middle and its
/// width (the rows', the headings', the note's; the scroll bar is right of it, the band
/// round both) [design: #2377].
#[derive(Clone, Copy, Debug, PartialEq)]
struct ListBox {
    left: f32,
    width: f32,
}

impl ListBox {
    /// The list in a view `view_w` units wide whose rail is `tabs_w` wide: as wide as the room
    /// left of the rail allows (the band [`LEFT_MARGIN`] from the view's left edge and
    /// [`TABS_GAP`] clear of the rail), at most [`OPTIONS_MAX`], then centred in that room.
    /// A narrow view gets a narrow list rather than one that runs under the rail (#1210).
    fn of(view_w: f32, tabs_w: f32) -> ListBox {
        let room = (view_w - (SUB_RIGHT + tabs_w + TABS_GAP) - (BAR_GAP + BAR_W) - 2.0 * BAND_MARGIN - LEFT_MARGIN).max(0.0);
        let width = room.min(OPTIONS_MAX);
        ListBox { left: -view_w / 2.0 + LEFT_MARGIN + BAND_MARGIN + (room - width) / 2.0, width }
    }

    /// The places in its rows.
    fn row(self) -> RowX {
        RowX(self.width)
    }

    /// The band behind the list and its scroll bar: its left edge from the view's middle and
    /// its width.
    fn band(self) -> (f32, f32) {
        (self.left - BAND_MARGIN, self.width + 2.0 * BAND_MARGIN + BAR_GAP + BAR_W)
    }
}

/// The places in a settings row `.0` units wide, from its left edge: each as far from the
/// row's right end as in a [`ROW_BASE`] row (module notes of the constants) [design: #2377].
#[derive(Clone, Copy, Debug, PartialEq)]
struct RowX(f32);

impl RowX {
    fn kept_from_right(self, at: f32) -> f32 {
        self.0 - (ROW_BASE - at)
    }

    /// A switch's state's and a reason's centre.
    fn value_at(self) -> f32 {
        self.kept_from_right(VALUE_AT)
    }

    /// The left edge of the row's value part: a slider's left arrow, a list's box.
    fn value_left(self) -> f32 {
        self.kept_from_right(LIST_LEFT)
    }

    fn meter_left(self) -> f32 {
        self.kept_from_right(METER_LEFT)
    }

    fn arrows(self) -> [f32; 2] {
        METER_ARROWS.map(|a| self.kept_from_right(a))
    }

    /// A slider's value's centre.
    fn percent_at(self) -> f32 {
        self.kept_from_right(PERCENT_AT)
    }

    fn list_left(self) -> f32 {
        self.kept_from_right(LIST_LEFT)
    }
}

/// `text` as it fits in `room` units measured by `width_of` (a face's font): itself, or cut
/// at the end with [`CUT_MARK`]; only the mark, or nothing, where even that does not fit.
/// What is cut is only what is drawn: the search, the log and the hidden texts the aids read
/// keep the full words [design: #2377].
fn fit_text(width_of: impl Fn(&str) -> f32, text: &str, room: f32) -> String {
    if width_of(text) <= room {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while chars.pop().is_some() {
        let cut = format!("{}{CUT_MARK}", chars.iter().collect::<String>().trim_end());
        if !chars.is_empty() && width_of(&cut) <= room {
            return cut;
        }
    }
    if width_of(CUT_MARK) <= room {
        CUT_MARK.to_string()
    } else {
        String::new()
    }
}

/// A row's value, for its layout ([`row_parts`]): what it is and its words' measured width.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ValueKind {
    /// A switch's state or a reason, centred at [`RowX::value_at`].
    Centred(f32),
    /// A slider's meter and arrows, its value's words right of them.
    Slider(f32),
    /// A list's box, the chosen one's words in it.
    List(f32),
    /// A Controls binding: its keys' words (cut like other words where they do not fit) or a
    /// button's picture.
    Binding(f32),
}

/// Where a row's label and value are drawn, from the row's left edge: the one place that
/// lays a row out, for the drawing and the checks ([`check_page_text`]) [design: #2377].
#[derive(Clone, Copy, Debug, PartialEq)]
struct RowParts {
    /// The label's room: from its left edge to where it must end, [`LABEL_GAP`] before the
    /// value.
    label: (f32, f32),
    /// The value's part of the row.
    value: (f32, f32),
    /// The room of the value's words and the point they are centred on (kept inside the
    /// room); words wider than the room are cut.
    text: (f32, f32),
    centre: f32,
    /// A slider's meter (its left edge and width) and its right arrow's left edge (the
    /// meter gives room to wide words).
    meter: (f32, f32),
    right_arrow: f32,
}

impl RowParts {
    /// Where words `w` wide (cut to the room first) start.
    fn text_x(&self, w: f32) -> f32 {
        (self.centre - w / 2.0).clamp(self.text.0, (self.text.1 - w).max(self.text.0))
    }

    fn text_room(&self) -> f32 {
        (self.text.1 - self.text.0).max(0.0)
    }

    fn label_room(&self) -> f32 {
        (self.label.1 - self.label.0).max(0.0)
    }
}

/// The parts of a row laid out at `x` for a value of `kind` ([`RowParts`]): the value's
/// place as before #2377 where its words fit, a wide reason or binding taking room from the
/// label down to [`LABEL_MIN`], wide slider words from the meter down to [`METER_MIN`].
fn row_parts(x: RowX, kind: ValueKind) -> RowParts {
    let w = x.0;
    let label_left = 10.0;
    let fixed = x.value_left();
    // A wide value never starts left of this (unless the row is too narrow for its usual
    // value part already).
    let floor = (label_left + LABEL_MIN + LABEL_GAP).min(fixed);
    let right = w - VALUE_PAD;
    let [left_arrow, right_arrow] = x.arrows();
    let (value, text, centre, meter_w, right_arrow) = match kind {
        ValueKind::Centred(vw) => {
            let left = fixed.min(right - vw).max(floor);
            ((left, right), (left, right), x.value_at(), METER_WIDTH, right_arrow)
        }
        ValueKind::Slider(vw) => {
            let room = right - (right_arrow + METER_ARROW.x + ARROW_PAD);
            let extra = (vw - room).clamp(0.0, METER_WIDTH - METER_MIN);
            let arrow = right_arrow - extra;
            let text = (arrow + METER_ARROW.x + ARROW_PAD, right);
            let centre = if extra > 0.0 { (text.0 + text.1) / 2.0 } else { x.percent_at() };
            ((left_arrow, right), text, centre, METER_WIDTH - extra, arrow)
        }
        ValueKind::List(_) => {
            let left = x.list_left();
            let text = (left + VALUE_PAD, left + LIST_WIDTH - 30.0 - VALUE_PAD);
            ((left, left + LIST_WIDTH), text, left + (LIST_WIDTH - 30.0) / 2.0, METER_WIDTH, right_arrow)
        }
        ValueKind::Binding(vw) => {
            // Within the row: words wider than from `floor` to its right end are cut.
            let left = (x.value_at() - vw / 2.0).min(w - vw - 14.0).max(floor);
            let text = (left, (left + vw).min(right));
            ((left.min(fixed), right), text, (text.0 + text.1) / 2.0, METER_WIDTH, right_arrow)
        }
    };
    RowParts { label: (label_left, (value.0 - LABEL_GAP).max(label_left)), value, text, centre, meter: (x.meter_left(), meter_w), right_arrow }
}

/// A row's label and value words as drawn in `parts`, each cut to its room where it does
/// not fit.
fn fit_row(width_of: impl Fn(&str) -> f32 + Copy, parts: &RowParts, label: &str, value: &str) -> (String, String) {
    (fit_text(width_of, label, parts.label_room()), fit_text(width_of, value, parts.text_room()))
}

/// The settings page's scroll bar, right of its window of `lines` rows.
fn scroll_bar(root: &mut ChildSpawnerCommands, look: &PauseLook, rows: usize, first: usize, top: f32, lb: ListBox, lines: usize) {
    let w = settings_window(lines);
    let bar = from_middle(lb.left + lb.width + BAR_GAP, top, BAR_W, w.shown as f32 * w.row);
    window_bar(root, look, rows, first, w, bar);
}

/// A list's scroll bar in `bar` when not all `rows` fit in `w`, from `first`: its arrows
/// step the list, a press or drag on its track puts the marker there.
fn window_bar(root: &mut ChildSpawnerCommands, look: &PauseLook, rows: usize, first: usize, w: Window, bar: Node) {
    let Some((y, h)) = w.marker(rows, first) else { return };
    let bright = color(Shade::Bright);
    let track = w.track();
    let piece = |p: &mut ChildSpawnerCommands, k: usize, node: Node, b: Option<PauseButton>| {
        let mut e = match &look.bar[k] {
            Some((image, rect)) => p.spawn((node, ImageNode { rect: *rect, color: bright, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) })),
            None => p.spawn((node, BackgroundColor(bright.with_alpha(if k == 2 { 1.0 } else { 0.5 })))),
        };
        if let Some(b) = b {
            e.insert((Button, b));
        }
    };
    root.spawn(bar).with_children(|p| {
        piece(p, 0, at(0.0, 0.0, BAR_W, BAR_ARROW), Some(PauseButton::ScrollStep(-1)));
        p.spawn((Button, PauseButton::ScrollTrack, RelativeCursorPosition::default(), at(0.0, BAR_ARROW, BAR_W, track))).with_children(|t| piece(t, 2, at(12.0, y, 6.0, h), None));
        piece(p, 1, at(0.0, BAR_ARROW + track, BAR_W, BAR_ARROW), Some(PauseButton::ScrollStep(1)));
    });
}

/// The version at the bottom left [observed: the original shows its own there].
fn version(root: &mut ChildSpawnerCommands, f: &Faces) {
    root.spawn(Node { position_type: PositionType::Absolute, left: u(30.0), top: u(960.0 - 60.0), width: u(400.0), height: u(40.0), ..default() }).with_children(|t| {
        sm_text(t, &f.small, concat!("NVForever ", env!("CARGO_PKG_VERSION")), 0.0, 26.0, color(Shade::Bright));
    });
}

/// "Report a bug" just over [`version`]'s line, left-aligned with it; boxed unfilled while
/// the pointer is on it, as the rail's lines.
fn report_entry(root: &mut ChildSpawnerCommands, f: &Faces, hovered: bool, glyphs: &crate::pad::PadGlyphs) {
    let label = tr(REPORT_LABEL);
    let w = crate::pad::hint_width(&f.entry, label, glyphs) + 20.0;
    let node = Node { position_type: PositionType::Absolute, left: u(20.0), top: u(REPORT_TOP), width: u(w), height: u(ITEM), ..default() };
    let look = if hovered { BoxLook::Outline } else { BoxLook::None };
    right_entry_look(root, &f.entry, label, node, w, if hovered { Shade::Bright } else { Shade::Dim }, look, PauseButton::Report, glyphs);
}

/// A node at `x`, `y` in its parent, `w` × `h`.
fn at(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node { position_type: PositionType::Absolute, left: u(x), top: u(y), width: u(w), height: u(h), ..default() }
}

/// How many ticks of a slider's meter `meter_w` wide show for value `v` (a tick partly in
/// shows, cut).
fn meter_ticks(v: f32, meter_w: f32) -> usize {
    (meter_w * v.clamp(0.0, 1.0) / METER_TICK).ceil() as usize
}

/// A slider's meter, arrows and value (`value`: `v` as shown, e.g. a percentage) in its
/// row `i`, laid out by `parts` (wide words take room from the meter, then are cut).
fn slider_row(r: &mut ChildSpawnerCommands, look: &PauseLook, parts: &RowParts, i: RowId, v: f32, value: &str) {
    let bright = color(Shade::Bright);
    let my = (OPTION_ROW - METER_HEIGHT) / 2.0;
    let (meter_left, meter_w) = parts.meter;
    // The ground, then the ticks as far as the value.
    r.spawn((at(meter_left, my, meter_w, METER_HEIGHT), BackgroundColor(bright.with_alpha(0.25))));
    for t in 0..meter_ticks(v, meter_w) {
        let x = meter_left + t as f32 * METER_TICK;
        let w = METER_TICK.min(meter_left + meter_w - x);
        match &look.tick {
            Some(image) => r.spawn((at(x, my, w, METER_HEIGHT), ImageNode { rect: Some(Rect::new(0.0, 0.0, w, 20.0)), color: bright, ..ImageNode::new(image.clone()) })),
            None => r.spawn((at(x, my, (w - 3.0).max(1.0), METER_HEIGHT), BackgroundColor(bright))),
        };
    }
    // Over them what takes the pointer, a little taller than the meter.
    r.spawn((Button, PauseButton::Meter(i), RelativeCursorPosition::default(), at(meter_left, my - 6.0, meter_w, METER_HEIGHT + 12.0)));
    for (k, (x, step)) in [(parts.value.0, -1), (parts.right_arrow, 1)].into_iter().enumerate() {
        let node = at(x, (OPTION_ROW - METER_ARROW.y) / 2.0, METER_ARROW.x, METER_ARROW.y);
        match &look.arrows[k] {
            Some(image) => r.spawn((Button, PauseButton::SliderStep(i, step), node, ImageNode { color: bright, ..ImageNode::new(image.clone()) })),
            None => r.spawn((Button, PauseButton::SliderStep(i, step), node, BackgroundColor(bright.with_alpha(0.5)))),
        };
    }
    r.spawn((SwitchState(i), Text::new(value), Node { display: Display::None, ..default() }));
    // A value with words ("Off" on the frame rate limit) in the language shown.
    let font = &look.faces.entry.font;
    let value = fit_text(|t| font.width(t), tr_str(value), parts.text_room());
    sm_text(r, &look.faces.entry, &value, parts.text_x(font.width(&value)), BASE + 2.0, bright);
}

/// The load page: the chosen save's thumbnail and details over the saves' rows (a window
/// of [`LOAD_ROWS`] from [`PauseView::scroll`], moved by the wheel, the scroll bar or the
/// keys, never by the pointer), left of the column; the scroll bar at the rows' right
/// when not all of them fit.
fn load_page(root: &mut ChildSpawnerCommands, d: &Drawn, look: &PauseLook, list: &LoadList, selected: usize, glyphs: &crate::pad::PadGlyphs) {
    let f = &look.faces;
    let panel = |top: f32, height: f32| from_right(RIGHT + COLUMN, top, LOAD_WIDTH, height);
    if list.saves.is_empty() {
        root.spawn(panel(LOAD_TOP, ITEM)).with_children(|t| sm_text(t, &f.entry, "No saves yet.", 10.0, BASE, color(Shade::Dim)));
        return;
    }
    let n = list.saves.len();
    let first = crate::menu::model::keep_in_view(d.scroll, selected, n, LOAD_ROWS);
    // The thumbnail, stretched to its frame.
    let frame = Node { position_type: PositionType::Absolute, right: u(RIGHT + COLUMN + LOAD_WIDTH - THUMB_DRAWN.x - 10.0), top: u(DETAILS_TOP - THUMB_DRAWN.y - 10.0), width: u(THUMB_DRAWN.x), height: u(THUMB_DRAWN.y), ..default() };
    match list.thumb.as_ref().filter(|(t, _)| *t == selected && d.thumb.is_some_and(|t| t.1)).and_then(|(_, h)| h.clone()) {
        Some(img) => {
            root.spawn((frame, ImageNode::new(img).with_mode(NodeImageMode::Stretch)));
        }
        None => {
            root.spawn((frame, BackgroundColor(Color::BLACK.with_alpha(0.5)))).with_children(|t| sm_text(t, &f.small, "No picture", 20.0, 40.0, color(Shade::Dim)));
        }
    }
    if let Some((path, h)) = list.saves.get(selected) {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let place = if h.location.is_empty() { "Unknown place".to_string() } else { h.location.clone() };
        let lines = [
            format!("{}, level {}", h.player_name, h.level),
            format!("{place}, {}", crate::save::game_date(h)),
            format!("Played {}", play_time(h.play_seconds)),
            format!("{}: saved {} ({})", stem(path), saved_ago(h.saved_unix, now), kind_name(h.kind)),
        ];
        for (i, line) in lines.iter().enumerate() {
            let face = if i == 0 { &f.entry } else { &f.small };
            root.spawn((DetailLine, Text::new(line.clone()), Node { display: Display::None, ..default() }));
            root.spawn(panel(DETAILS_TOP + i as f32 * DETAILS_STEP, DETAILS_STEP)).with_children(|t| sm_text(t, face, line, 10.0, 30.0, color(Shade::Bright)));
        }
    }
    // Delete, right of the thumbnail; asked, the question and its answers under it.
    let side = |top: f32, height: f32| from_right(RIGHT + COLUMN, top, DELETE_WIDTH, height);
    let top = DETAILS_TOP - THUMB_DRAWN.y - 10.0;
    match d.delete_ask.filter(|r| *r == selected) {
        None => right_entry(root, &f.small, &format!("Delete  {}", crate::pad::hint(&[KeyCode::Delete], crate::pad::PAUSE_AT)), side(top, ITEM), DELETE_WIDTH, Shade::Bright, false, PauseButton::Delete, glyphs),
        Some(_) => {
            root.spawn((DetailLine, Text::new("Delete this save?"), Node { display: Display::None, ..default() }));
            root.spawn(side(top, ITEM)).with_children(|t| {
                let w = f.small.font.width("Delete this save?");
                sm_text(t, &f.small, "Delete this save?", DELETE_WIDTH - w - 10.0, BASE, color(Shade::Bright));
            });
            let (yes, no) = (crate::pad::hint(&[KeyCode::KeyY, KeyCode::Enter], crate::pad::PAUSE_AT), crate::pad::hint(&[KeyCode::KeyN, KeyCode::Escape], crate::pad::PAUSE_AT));
            right_entry(root, &f.small, &format!("Yes  {yes}"), side(top + ITEM, ITEM), DELETE_WIDTH, Shade::Bright, true, PauseButton::DeleteAnswer(true), glyphs);
            right_entry(root, &f.small, &format!("No  {no}"), side(top + 2.0 * ITEM, ITEM), DELETE_WIDTH, Shade::Bright, false, PauseButton::DeleteAnswer(false), glyphs);
        }
    }
    // With the scroll bar the rows leave room for it at their right.
    let bar = LOAD_WINDOW.marker(n, first).is_some();
    let width = if bar { LOAD_WIDTH - BAR_W - BAR_GAP } else { LOAD_WIDTH };
    window_bar(root, look, n, first, LOAD_WINDOW, from_right(RIGHT + COLUMN, LOAD_TOP, BAR_W, LOAD_WINDOW.shown as f32 * LOAD_WINDOW.row));
    for (row, (i, (path, h))) in list.saves.iter().enumerate().skip(first).take(LOAD_ROWS).enumerate() {
        let node = from_right(RIGHT + COLUMN + LOAD_WIDTH - width, LOAD_TOP + row as f32 * ITEM, width, ITEM);
        root.spawn((Button, PauseButton::LoadRow(i), boxed(node, i == selected))).with_children(|r| {
            sm_text(r, &f.entry, &stem(path), 10.0, BASE, color(Shade::Bright));
            let date = format!("{}  {}", kind_name(h.kind), crate::save::game_date(h));
            let w = f.small.font.width(&date);
            sm_text(r, &f.small, &date, width - w - 14.0, BASE, color(Shade::Dim));
        });
    }
}

/// A line of the details, kept as text too (tests read it).
#[derive(Component)]
struct DetailLine;


#[allow(clippy::too_many_arguments)]
fn open(
    keys: crate::pad::ActionButtons<KeyCode>,
    time: AidTime,
    mut aid: ResMut<PauseAid>,
    mut paused: ResMut<Paused>,
    mut page: ResMut<Page>,
    mut view: ResMut<PauseView>,
    mut status: ResMut<PauseStatus>,
    mut next: ResMut<NextState<PlayState>>,
    wheel: Option<Res<crate::weapon_wheel::WeaponWheel>>,
    (script, mut seen, mut request): (Option<Res<crate::script::ScriptState>>, Local<u64>, ResMut<PageRequest>),
) {
    let keys = keys.snapshot();
    let by_aid = fire(&mut aid.open, time.elapsed_secs());
    // ShowOff's ShowPauseMenu: each new ask opens the menu, on the page its mode names
    // (1 load, 3 settings; the others the main page) (`fnv_game::nvse_showoff5`).
    let asked = script.as_ref().and_then(|s| s.rt.as_ref()).map(|rt| (rt.game.showoff.b5.pause_asks, rt.game.showoff.b5.pause_mode));
    let by_script = match asked {
        Some((n, mode)) if n != *seen => {
            *seen = n;
            (n > 0 && !paused.0).then_some(mode)
        }
        _ => None,
    };
    if !keys.just_pressed(KeyCode::Escape) && !by_aid && by_script.is_none() {
        return;
    }
    if let Some(mode) = by_script {
        info!("pause menu: opened by ShowPauseMenu {mode}");
        request.0 = match mode {
            1 => Some(Page::Load { selected: 0 }),
            3 => Some(Page::Settings { selected: None }),
            _ => None,
        };
    }
    // Esc closes the weapon wheel (`crate::weapon_wheel`), not the game.
    if wheel.is_some_and(|w| w.holds_input()) && !by_aid && by_script.is_none() {
        return;
    }
    if by_aid {
        info!("test aid NVF_PLAY_TEST_PAUSE: Esc");
    }
    info!("pause menu: open");
    paused.0 = true;
    *page = Page::Main;
    view.main_selected = 0;
    view.confirm = None;
    *status = PauseStatus::default();
    next.set(PlayState::Menu);
}

/// A page another menu asks the pause menu to show (the main menu's Load Game and
/// Settings, `crate::main_menu`); taken on the next frame.
#[derive(Resource, Default)]
pub struct PageRequest(pub Option<Page>);

/// Asked from the quit question: back to the main menu (nvforever starts again with
/// `--menu`).
#[derive(Message, Clone, Copy, Debug)]
pub struct MainMenuRequest;

/// The row after `selected` (`step` 1) or before it (-1) of `rows`, round the ends.
fn step_row(selected: usize, step: i32, rows: usize) -> usize {
    if rows == 0 {
        return 0;
    }
    (selected.min(rows - 1) as i32 + step).rem_euclid(rows as i32) as usize
}

/// The keys pressed this frame, with those the test aids press ([`PauseAid`]'s settings
/// verbs, `pad:<button>`): the aids go the keys' way.
struct Pressed<'a> {
    keys: &'a ButtonInput<KeyCode>,
    extra: Vec<KeyCode>,
}

impl Pressed<'_> {
    fn any(&self, ks: impl IntoIterator<Item = KeyCode>) -> bool {
        ks.into_iter().any(|k| self.keys.just_pressed(k) || self.extra.contains(&k))
    }

    fn just(&self, k: KeyCode) -> bool {
        self.any([k])
    }

    fn shift(&self) -> bool {
        self.keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
    }

    fn ctrl(&self) -> bool {
        self.keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
    }
}

/// What a row is asked to do: the one way the keys, the pointer and the test aids act on
/// a row of the settings page, whatever lists it (a tab; the search and the Changed list
/// too) [design: #1953].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowAction {
    /// Enter, Space, A or a click on the row: a switch flips, a list opens, a control waits
    /// for its new key or button; a slider only is chosen.
    Activate,
    /// Left (-1) or Right (1), or a slider's arrow: a switch flips, a slider steps, a list
    /// takes the choice beside its own.
    Step(i32),
    /// Delete (a controller's X): a control's key or button is cleared.
    Clear,
}

/// The registered rows' changes asked this frame: taken after the frame's keys, in this
/// order, as before #1953 (the test aids' order within a frame depends on it).
#[derive(Default)]
struct RowEffects {
    flips: Vec<usize>,
    steps: Vec<(usize, i32)>,
    sets: Vec<(usize, f32)>,
    picks: Vec<(usize, String)>,
}

/// The controls' state a Controls row's action needs.
#[derive(Clone, Copy, Debug)]
struct ControlState {
    device: crate::bindings::InputDevice,
    automatic_input: bool,
    rumble: bool,
    invert: bool,
    sensitivity: f32,
    vertical: f32,
}

/// Whether row `id` takes A and D as Left and Right (a slider, a list; a switch takes the
/// arrows only, as before).
fn takes_letters(settings: &Settings, id: RowId) -> bool {
    match id {
        RowId::Setting(i) => settings.choices(i).is_some() || settings.list().get(i).is_some_and(|s| s.slider.is_some()),
        RowId::Control(c) => matches!(c, ControlRow::Sensitivity(_) | ControlRow::Vertical(_)),
    }
}

/// Do `action` on row `id` ([`RowAction`]).
#[allow(clippy::too_many_arguments)]
fn act_on_row(id: RowId, action: RowAction, settings: &Settings, view: &mut PauseView, fx: &mut RowEffects, controls: ControlState, listening: &mut Listening, commands: &mut Commands, sounds: &mut MessageWriter<crate::ui_sounds::UiSound>) {
    use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
    use RowAction::{Activate, Clear, Step};
    let flipped = UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped };
    match id {
        RowId::Setting(i) if settings.choices(i).is_some() => match action {
            Activate => {
                open_dropdown(view, i);
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Picked });
            }
            Step(s) => {
                if let Some(name) = neighbour(view, i, s) {
                    fx.picks.push((i, name));
                }
            }
            Clear => {}
        },
        RowId::Setting(i) if settings.list().get(i).is_some_and(|s| s.slider.is_some()) => {
            if let Step(s) = action {
                fx.steps.push((i, s));
            }
        }
        RowId::Setting(i) => {
            if action != Clear && i < settings.list().len() {
                fx.flips.push(i);
            }
        }
        RowId::Control(c) => match (c, action) {
            (ControlRow::AutomaticInput, Activate | Step(_)) => {
                let on = !controls.automatic_input;
                commands.queue(move |world: &mut World| crate::bindings::set_automatic_input_switching(world, on));
                sounds.write(flipped);
            }
            (ControlRow::Device, Activate | Step(_)) => {
                let d = controls.device.other();
                commands.queue(move |world: &mut World| crate::bindings::set_device(world, d));
                sounds.write(flipped);
            }
            (ControlRow::Binding(k, _), Activate) => {
                listening.0 = Some((k, controls.device));
                let what = if controls.device == crate::bindings::InputDevice::Controller { "controller button" } else { "key" };
                info!("controls: {} waits for its new {what}", crate::bindings::Bindings::name(k));
            }
            (ControlRow::Binding(k, _), Clear) => {
                commands.queue(move |world: &mut World| crate::bindings::clear_key(world, k));
                sounds.write(flipped);
            }
            (ControlRow::Invert, Activate | Step(_)) => {
                let on = !controls.invert;
                commands.queue(move |world: &mut World| crate::bindings::set_invert_y(world, on));
                sounds.write(flipped);
            }
            (ControlRow::Rumble, Activate | Step(_)) => {
                let on = !controls.rumble;
                commands.queue(move |world: &mut World| crate::bindings::set_rumble(world, on));
                sounds.write(flipped);
            }
            (ControlRow::Sensitivity(_), Step(s)) => {
                let m = crate::bindings::step_sensitivity(controls.sensitivity, s);
                commands.queue(move |world: &mut World| crate::bindings::set_sensitivity(world, m));
                sounds.write(flipped);
            }
            (ControlRow::Vertical(_), Step(s)) => {
                let m = crate::bindings::step_sensitivity(controls.vertical, s);
                commands.queue(move |world: &mut World| crate::bindings::set_vertical(world, m));
                sounds.write(flipped);
            }
            _ => {}
        },
    }
}

/// Show what `scope` lists on its first row (the scope's window from its top when it is
/// another than the one shown).
fn show_scope(page: &mut Page, view: &mut PauseView, scope: SettingsScope) {
    if view.scope != scope || !matches!(*page, Page::Settings { .. }) {
        view.scroll = 0;
    }
    if scope != SettingsScope::Search {
        close_search(view);
    }
    view.scope = scope;
    view.focus = SettingsFocus::Rows;
    *page = Page::Settings { selected: None };
}

/// Show row `id` chosen, on its tab.
fn show_row(page: &mut Page, view: &mut PauseView, layout: &ResolvedLayout, id: RowId) {
    show_scope(page, view, SettingsScope::Tab(crate::settings_view::home_tab(layout, id)));
    *page = Page::Settings { selected: Some(id) };
}

/// The settings page's Defaults: on the Changed list exactly its rows, on the Controls tab
/// the controls' keys (or buttons) and the tab's registered rows, elsewhere every
/// registered row (module notes).
fn defaults(view: &PauseView, layout: &ResolvedLayout, list: &[Setting], commands: &mut Commands, sounds: &mut MessageWriter<crate::ui_sounds::UiSound>) {
    use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
    if view.scope == SettingsScope::Changed {
        changed_defaults(view, list, commands);
        sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
        return;
    }
    if view.scope == SettingsScope::Tab(CONTROLS_TAB) {
        commands.queue(crate::bindings::reset);
        if let Some(d) = view.defaults.clone() {
            let rows: Vec<usize> = layout.tab(CONTROLS_TAB).map(|t| t.sections.iter().flat_map(|s| s.rows.iter().copied()).collect()).unwrap_or_default();
            commands.queue(move |world: &mut World| {
                for i in rows {
                    if let Some(v) = d.get(i) {
                        crate::settings::restore(world, i, *v);
                    }
                }
            });
        }
        sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
    } else if let Some(d) = view.defaults.clone() {
        info!("pause menu: settings back to their defaults");
        commands.queue(move |world: &mut World| {
            for (i, v) in d.into_iter().enumerate() {
                crate::settings::restore(world, i, v);
            }
        });
        sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
    }
}

/// Defaults on the Changed list: the rows it lists now, hidden ones too, each put back as
/// Defaults would (`crate::settings::restore`; the Controls rows by
/// `crate::bindings::restore_rows`, without a swap), no other row touched [design: #1953
/// point 6]. Twice over the registered rows, so a row another one held disabled (the
/// anti-aliasing under FSR) goes back once that one has; a row still disabled stays as it
/// is and stays listed.
fn changed_defaults(view: &PauseView, list: &[Setting], commands: &mut Commands) {
    let ids = view.found.changed.clone();
    info!("pause menu: changed defaults; keys {}", ids.iter().map(|id| id.name(list)).collect::<Vec<_>>().join(","));
    let d = view.defaults.clone().unwrap_or_default();
    let rows: Vec<usize> = ids
        .iter()
        .filter_map(|id| match id {
            RowId::Setting(i) => Some(*i),
            RowId::Control(_) => None,
        })
        .collect();
    let controls: Vec<ControlRow> = ids
        .iter()
        .filter_map(|id| match id {
            RowId::Control(c) => Some(*c),
            RowId::Setting(_) => None,
        })
        .collect();
    commands.queue(move |world: &mut World| {
        for _ in 0..2 {
            for &i in &rows {
                if let Some(v) = d.get(i) {
                    crate::settings::restore(world, i, *v);
                }
            }
        }
        crate::bindings::restore_rows(world, &controls);
    });
}

/// Open the settings page's search with the keys in its text, noting what was listed for
/// Esc to go back to; on the search already, the keys go back to its text [design: #1953].
fn open_search(view: &mut PauseView, selected: Option<RowId>) {
    if view.search.from.is_none() {
        view.search = SearchState { from: Some((view.scope, selected, view.scroll)), ..default() };
        view.scroll = 0;
        info!("pause menu: search open");
    }
    view.scope = SettingsScope::Search;
    view.focus = SettingsFocus::SearchText;
}

/// The settings page opened from the column (or the main menu, a script): where it was
/// left this run, its row and first line ([`PauseView::last`]); the first time, on the tab
/// the INI keeps (`kept`, [`SettingsTabStore`]) or the first tab [design: #1953, the fork
/// owner's request 2026-11-09]. A remembered row no longer listed gives way to the tab's
/// first row (`crate::settings_view::normalize`).
fn open_settings(page: &mut Page, view: &mut PauseView, kept: Option<&'static str>) {
    let (scope, selected, scroll) = view.last.unwrap_or((kept.map_or_else(SettingsScope::default, SettingsScope::Tab), None, 0));
    show_scope(page, view, scope);
    view.scroll = scroll;
    // The window as it was left: its row counts as followed, so it is not pulled back up
    // to its heading (a row gone since is followed anew by `normalize`).
    view.followed = selected;
    *page = Page::Settings { selected };
}

/// Note where the settings page is ([`PauseView::last`]): on the search, what it came
/// from (its text is not kept).
fn remember(view: &mut PauseView, selected: Option<RowId>) {
    view.last = Some(match view.search.from {
        Some(from) if view.scope == SettingsScope::Search => from,
        _ => (view.scope, selected, view.scroll),
    });
}

/// The INI section and key the settings page's tab is kept under, beside the other UI
/// switches (`[UI] TileMenus`, `crate::tiles::vanilla`) [design: #1953].
pub const TAB_SECTION: &str = crate::tiles::vanilla::SECTION;
pub const TAB_KEY: &str = "SettingsTab";

/// The settings page's tab kept across runs (Continue and a save's load start NVForever
/// again): read once at start from `[UI] SettingsTab=` of the INI the settings page uses
/// (`crate::settings::SettingsFile`: an unattended run in the player's own home reads and
/// writes none), written when the tab shown changes, by its English name; an unknown or
/// missing name opens the first tab [design: #1953].
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct SettingsTabStore {
    pub path: Option<PathBuf>,
    pub tab: Option<String>,
}

impl SettingsTabStore {
    /// The tab `t` shown: kept in the INI when it is not the one kept already.
    fn keep(&mut self, t: &str) {
        if self.tab.as_deref() == Some(t) {
            return;
        }
        self.tab = Some(t.to_string());
        let Some(path) = self.path.clone() else { return };
        match write_tab(&path, t) {
            Ok(()) => info!("settings page: [{TAB_SECTION}] {TAB_KEY}={t} kept in {}", path.display()),
            Err(e) => warn!("settings page: [{TAB_SECTION}] {TAB_KEY} not kept: {e}"),
        }
    }
}

/// `[UI] SettingsTab=<tab>` written into the INI at `path`, its other lines kept.
fn write_tab(path: &Path, tab: &str) -> Result<(), String> {
    fnv_install::guard_write(path, None).map_err(|e| e.to_string())?;
    let old = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, fnv_install::ini_set(&old, TAB_SECTION, TAB_KEY, tab)).map_err(|e| format!("{}: {e}", path.display()))
}

/// Read the kept tab at start (module notes).
fn read_settings_tab(file: Option<Res<crate::settings::SettingsFile>>, mut store: ResMut<SettingsTabStore>) {
    let Some(path) = file.and_then(|f| f.0.clone()) else { return };
    let tab = std::fs::read_to_string(&path).ok().and_then(|t| fnv_install::ini_get(&t, TAB_SECTION, TAB_KEY)).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    if let Some(t) = &tab {
        info!("settings page: [{TAB_SECTION}] {TAB_KEY}={t} read from {}", path.display());
    }
    *store = SettingsTabStore { path: Some(path), tab };
}

/// Esc, Back or the rail's Back on the search: the search closed and what was listed when
/// it opened shown again, its first line too, the keys on its rows; the row chosen then.
fn leave_search(view: &mut PauseView, sounds: &mut MessageWriter<crate::ui_sounds::UiSound>) -> Option<RowId> {
    use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
    let (scope, sel, scroll) = close_search(view).unwrap_or((SettingsScope::default(), None, 0));
    view.scope = scope;
    view.scroll = scroll;
    view.focus = SettingsFocus::Rows;
    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Back });
    sel
}

/// Close the search, its text cleared; what was listed when it opened (`None`: it was not
/// open).
fn close_search(view: &mut PauseView) -> Option<(SettingsScope, Option<RowId>, usize)> {
    let from = view.search.from.take();
    if from.is_some() {
        info!("pause menu: search closed");
    }
    view.search = SearchState::default();
    from
}

/// The search's rows for its text, worked out again when the text, the language or the
/// registry changed.
fn refresh_search(view: &mut PauseView, layout: &ResolvedLayout, list: &[Setting]) {
    let key = (view.search.query.clone(), fnv_util::words::generation(), list.len());
    if view.search.key.as_ref() != Some(&key) {
        view.found.search = crate::settings_view::search(layout, list, &view.search.query, control_words);
        view.search.key = Some(key);
    }
}

/// The search's words for a Controls row: its label and note as the page shows them for
/// the row's device, and its name in the INI (`Jump`, `Invert Y`) [design: #1953; the
/// rows are not registered settings, so this stands in for their label and note].
fn control_words(c: ControlRow) -> String {
    use crate::bindings::InputDevice;
    let controller = matches!(c, ControlRow::Binding(_, InputDevice::Controller) | ControlRow::Sensitivity(InputDevice::Controller) | ControlRow::Vertical(InputDevice::Controller) | ControlRow::Rumble);
    let d = Drawn { device: ("", controller), ..default() };
    let (label, _) = control_line(&d, c);
    let key = match c {
        ControlRow::Binding(k, _) => crate::bindings::Bindings::name(k),
        ControlRow::Device | ControlRow::AutomaticInput | ControlRow::Invert | ControlRow::Sensitivity(_) | ControlRow::Vertical(_) | ControlRow::Rumble => label,
    };
    let note = row_note(&[], RowId::Control(c), &d);
    crate::settings_view::haystack(&[label, note.as_str(), key])
}

/// The search's keys among `events` (pressed, repeats too), in their order: Backspace,
/// Enter, Esc, and the text of the others unless Ctrl, Alt or the logo key is held
/// (`modifiers`); a control character or text the game's fonts cannot draw is left out (as
/// the console's) [design: #1953].
fn search_keys(events: &[KeyboardInput], modifiers: bool) -> Vec<SearchKey> {
    events
        .iter()
        .filter(|k| k.state.is_pressed())
        .filter_map(|k| match &k.logical_key {
            Key::Backspace => Some(SearchKey::Backspace),
            Key::Enter => Some(SearchKey::Enter),
            Key::Escape => Some(SearchKey::Escape),
            _ if modifiers => None,
            _ => k.text.as_deref().filter(|t| !t.is_empty() && t.chars().all(|ch| !ch.is_control()) && fnv_util::text::encodable(t)).map(|t| SearchKey::Text(t.to_string())),
        })
        .collect()
}

/// Every frame, open or not, so nothing typed before the search opened comes in later:
/// the keyboard's messages read, the search's keys kept for the pause menu while the
/// search is open, and the keyboard held for it then ([`crate::bindings::KeyboardCapture`])
/// [design: #1953].
#[allow(clippy::too_many_arguments)]
fn collect_search_keys(
    (mut typed, switching): (MessageReader<KeyboardInput>, Option<Res<crate::pad::SwitchingInput>>),
    keys: crate::pad::ActionButtons<KeyCode>,
    paused: Option<Res<Paused>>,
    page: Option<Res<Page>>,
    view: Option<Res<PauseView>>,
    form: Option<Res<FormOver>>,
    mut out: ResMut<SearchKeys>,
    mut capture: ResMut<crate::bindings::KeyboardCapture>,
) {
    let keys = keys.snapshot();
    let events: Vec<KeyboardInput> = typed.read().filter(|e| switching.as_ref().is_none_or(|s| s.allows(e))).cloned().collect();
    let open = paused.is_some_and(|p| p.0) && page.is_some_and(|p| matches!(*p, Page::Settings { .. })) && view.is_some_and(|v| v.scope == SettingsScope::Search);
    let modifiers = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::AltLeft, KeyCode::AltRight, KeyCode::SuperLeft, KeyCode::SuperRight]);
    out.0 = if open { search_keys(&events, modifiers) } else { Vec::new() };
    capture.set_if_neq(crate::bindings::KeyboardCapture(open || form.is_some_and(|f| f.0)));
}

/// A key the pause menu aid presses: its code, what it types and its text.
fn aid_key(code: KeyCode, logical_key: Key, text: Option<&str>) -> KeyboardInput {
    KeyboardInput { key_code: code, logical_key, state: ButtonState::Pressed, text: text.map(Into::into), repeat: false, window: Entity::PLACEHOLDER }
}

/// `type:<text>`'s keys: each character's key with its text (an unknown key for a
/// character without one of its own).
fn typed_keys(text: &str) -> Vec<KeyboardInput> {
    text.chars()
        .map(|ch| {
            let t = ch.to_string();
            match ch {
                ' ' => aid_key(KeyCode::Space, Key::Space, Some(" ")),
                _ => {
                    let code = match crate::test_aid::real_input(&t) {
                        Ok((crate::test_aid::RealInput::Key(k), _)) => k,
                        _ => KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified),
                    };
                    aid_key(code, Key::Character(t.as_str().into()), Some(t.as_str()))
                }
            }
        })
        .collect()
}

/// The aid's keys as the window's messages, before Bevy reads them, so every reader of
/// the keys (the search's text, the keys' state, the game's controls) has them this frame;
/// each let go the next frame.
fn type_aid(mut aid: ResMut<PauseInputAid>, mut out: MessageWriter<KeyboardInput>, windows: Query<Entity, With<bevy::window::PrimaryWindow>>) {
    let aid = &mut *aid;
    if aid.keys.is_empty() && aid.up.is_empty() {
        return;
    }
    let window = windows.single().unwrap_or(Entity::PLACEHOLDER);
    for k in std::mem::take(&mut aid.up) {
        out.write(k);
    }
    for k in std::mem::take(&mut aid.keys) {
        let k = KeyboardInput { window, ..k };
        aid.up.push(KeyboardInput { state: ButtonState::Released, text: None, ..k.clone() });
        out.write(k);
    }
}

/// Whether the drawn button `b` is what `at` names.
fn points_at(at: &PointAt, b: PauseButton, list: &[Setting]) -> bool {
    match (at, b) {
        (PointAt::Button(x), b) => *x == b,
        (PointAt::Tab(name), PauseButton::Rail(RailItem::Tab(t))) => t.eq_ignore_ascii_case(name),
        (PointAt::Row(key), PauseButton::Row(id)) => id.name(list) == *key,
        (PointAt::Choice(k), PauseButton::Choice(_, c)) => c == *k,
        _ => false,
    }
}

/// `point:<target>`: `search`, `search-field`, `changed`, `defaults`, `back`,
/// `heading-up`, `heading-down`, `report` ("Report a bug"), `tab:<name>`, `row:<key>` (a Controls row by its name,
/// `bind:Jump`), `choice:<n>` (the open dropdown's, from 0).
fn parse_point(t: &str) -> Option<PointAt> {
    let t = t.trim();
    Some(match t {
        "search" => PointAt::Button(PauseButton::Rail(RailItem::Search)),
        "search-field" => PointAt::Button(PauseButton::SearchField),
        "changed" => PointAt::Button(PauseButton::Rail(RailItem::Changed)),
        "defaults" => PointAt::Button(PauseButton::Defaults),
        "back" => PointAt::Button(PauseButton::Back),
        "heading-up" => PointAt::Button(PauseButton::HeadingStep(-1)),
        "heading-down" => PointAt::Button(PauseButton::HeadingStep(1)),
        "report" => PointAt::Button(PauseButton::Report),
        _ => {
            if let Some(n) = t.strip_prefix("tab:") {
                PointAt::Tab(n.trim().to_string())
            } else if let Some(k) = t.strip_prefix("row:") {
                PointAt::Row(k.trim().to_string())
            } else {
                PointAt::Choice(t.strip_prefix("choice:")?.trim().parse().ok()?)
            }
        }
    })
}

/// The aid's pointer: onto the middle of the button drawn for its target (as laid out
/// last frame), and the left button pressed and let go, after Bevy read the real input
/// and before the UI looks under the pointer (as `crate::wait`'s pointer aid).
fn pointer_aid(
    mut aid: ResMut<PauseInputAid>,
    buttons: Query<(&PauseButton, &UiGlobalTransform, &ComputedNode)>,
    settings: Option<Res<Settings>>,
    mut windows: Query<&mut bevy::window::Window, With<bevy::window::PrimaryWindow>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
) {
    if aid.pointer.is_empty() {
        return;
    }
    let list = settings.as_ref().map_or(&[][..], |s| s.list());
    for step in std::mem::take(&mut aid.pointer) {
        match step {
            PointerStep::Point(name, at) => {
                let found = buttons.iter().find(|(b, _, n)| points_at(&at, **b, list) && n.size().x > 0.0).map(|(_, t, _)| t.translation);
                match (found, windows.single_mut()) {
                    (Some(p), Ok(mut w)) => {
                        w.set_physical_cursor_position(Some(p.as_dvec2()));
                        info!("pause menu: the pointer on {name}");
                    }
                    _ => warn!("NVF_PLAY_TEST_PAUSE_MENU: \"point:{name}\": nothing drawn there"),
                }
            }
            PointerStep::Press => mouse.press(MouseButton::Left),
            PointerStep::Release => mouse.release(MouseButton::Left),
        }
    }
}

/// `layout`: the tabs and every registered row's place, in the page's order.
fn log_layout(list: &[Setting], layout: &ResolvedLayout) {
    info!("settings layout: tabs {}", layout.tab_names().join(", "));
    for i in layout.ordered_settings() {
        if let (Some(s), Some(p)) = (list.get(i), layout.placement_of(i)) {
            info!("settings layout: {} -> {}", s.key, p.path());
        }
    }
    info!("settings layout: {} row(s) in {} tab(s), {} not in the table", list.len(), layout.tabs.len(), layout.unnamed());
}

/// `inspect`: what the settings page lists and shows (module notes).
fn log_inspect(list: &[Setting], view: &PauseView, selected: Option<RowId>, entries: &[ViewEntry], rail: &[RailItem], controller: bool) {
    let lines = view.lines();
    let first = view.scroll.min(entries.len().saturating_sub(lines));
    let drawn = crate::settings_view::drawn_slots(entries, first, lines);
    let focus = match view.focus {
        SettingsFocus::Rows => "rows".to_string(),
        SettingsFocus::Rail(r) => format!("rail ({})", r.name()),
        SettingsFocus::SearchText => "search text".to_string(),
    };
    let chosen = selected.map_or_else(|| "none".to_string(), |id| id.name(list));
    info!("pause menu: inspect: {}, keys on the {focus}, selected {chosen}, lines {}-{} of {}", view.scope.name(), drawn.start + usize::from(!drawn.is_empty()), drawn.end, entries.len());
    info!("pause menu: inspect: window {lines} lines, {} drawn", drawn.len());
    let line = |e: &ViewEntry| match e {
        ViewEntry::Heading(p) => format!("[{}]", p.path()),
        ViewEntry::Row(id) => id.name(list),
    };
    info!("pause menu: inspect: shown {}", entries[drawn].iter().map(line).collect::<Vec<_>>().join(", "));
    let rows: Vec<String> = entries.iter().filter(|e| matches!(e, ViewEntry::Row(_))).map(line).collect();
    info!("pause menu: inspect: rows {}", rows.join(","));
    // The words drawn: the rail's, the search's line and the headings' (translated), or
    // the empty list's.
    let mut words: Vec<String> = rail.iter().filter(|r| !matches!(r, RailItem::Defaults | RailItem::Back)).map(|r| rail_label(*r, controller).to_string()).collect();
    if view.scope == SettingsScope::Search {
        words.push(format!("{}: {}", tr(SEARCH), view.search.query));
    }
    let crumbs = !matches!(view.scope, SettingsScope::Tab(_));
    words.extend(entries.iter().filter_map(|e| match e {
        ViewEntry::Heading(p) => Some(heading_words(*p, crumbs)),
        ViewEntry::Row(_) => None,
    }));
    if entries.is_empty() {
        words.push(tr(empty_words(view.scope)).to_string());
    }
    info!("pause menu: inspect: words {}", words.join(" | "));
    info!("pause menu: inspect: hovered {}", view.rail_hover.map_or("none", |r| r.name()));
    info!("pause menu: inspect: back {}", back_label());
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn choose(
    keys: crate::pad::ActionButtons<KeyCode>,
    time: AidTime,
    mut aid: ResMut<PauseAid>,
    paused: Res<Paused>,
    mut page: ResMut<Page>,
    mut view: ResMut<PauseView>,
    settings: Res<Settings>,
    buttons: Query<(&PointerState, &PauseButton), Changed<PointerState>>,
    resume: Res<crate::graphics_resume::GraphicsResume>,
    mut exit: MessageWriter<AppExit>,
    mut commands: Commands,
    mut sounds: MessageWriter<crate::ui_sounds::UiSound>,
    mut menu: SaveMenu,
    (mut request, main_menu, intro, mut to_main, mouse, typed_in, mut input_aid, mut mods_page): (
        ResMut<PageRequest>,
        Option<Res<crate::main_menu::MainMenu>>,
        Option<Res<crate::intro::Intro>>,
        MessageWriter<MainMenuRequest>,
        crate::pad::ActionButtons<MouseButton>,
        Option<Res<SearchKeys>>,
        Option<ResMut<PauseInputAid>>,
        Option<ResMut<crate::tiles::mods::ModsPage>>,
    ),
    (bindings, pads, sticks, pending, mods, mut listening, mut layout, colours): (
        Option<Res<crate::bindings::Bindings>>,
        crate::pad::ActionPads,
        Option<Res<crate::pad::StickButtons>>,
        Option<Res<crate::settings::PendingKeep>>,
        Option<Res<ModsEntry>>,
        ResMut<Listening>,
        ResMut<SettingsLayout>,
        Option<Res<crate::ui_colour::CustomColours>>,
    ),
    (meters, pointed, mut tab_store, mut report): (Query<(&PauseButton, &RelativeCursorPosition)>, Query<(&PointerState, &PauseButton)>, Option<ResMut<SettingsTabStore>>, Option<ResMut<ReportEntry>>),
) {
    let keys = keys.snapshot();
    use crate::bindings::{Bindings, Input, InputDevice};
    use crate::pad::PadInput;
    use crate::settings_view as sv;
    use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
    // Held for the intro film, which takes the keys (`crate::intro`).
    if !paused.0 || intro.is_some_and(|i| i.is_playing()) {
        return;
    }
    view.set_lines();
    let lines = view.lines();
    // Under the main menu the first page is the main menu's own (`crate::main_menu`): its
    // pages come from here, and going back shows it again.
    let under_main = main_menu.as_ref().is_some_and(|m| m.open);
    let mods = mods.is_some_and(|m| m.0);
    let entries = entries(mods);
    // The Mods page goes when its entry does (the mod's tiles were unloaded).
    if *page == Page::Mods && !mods {
        info!("pause menu: no mod page any more, back to the column");
        *page = Page::Main;
    }
    // The tab kept in the INI, among the tabs there are now.
    let kept_tab = tab_store.as_ref().and_then(|t| t.tab.clone()).and_then(|t| layout.refresh(settings.list()).tab_names().into_iter().find(|n| *n == t));
    let mut log_open = false;
    if let Some(p) = request.0.take() {
        if matches!(p, Page::Load { .. }) {
            menu.list.open(menu.cfg.dir.as_deref());
        }
        info!("pause menu: {p:?} page for another menu");
        *page = p;
        if let Page::Settings { selected } = p {
            open_settings(&mut page, &mut view, kept_tab);
            if selected.is_some() {
                *page = Page::Settings { selected };
            }
            log_open = true;
        }
    }
    if under_main && *page == Page::Main {
        return;
    }
    let now = time.elapsed_secs();
    let by_aid = fire(&mut aid.resume, now);
    if by_aid {
        info!("test aid NVF_PLAY_TEST_PAUSE: Esc (resume)");
        // As Esc back to the column and Continue there: a wait only holds on the bare
        // column (`finish_resume`), and the release would close the page anyway.
        *page = Page::Main;
        view.confirm = None;
        view.dropdown = None;
        view.delete_ask = None;
        if !resume.pending() {
            commands.queue(request_resume);
        }
        return;
    }
    let list = settings.list();
    let layout = layout.refresh(list);
    let device = bindings.as_ref().map_or(InputDevice::KeyboardMouse, |b| b.device);
    let vis = Visibility::of(device, colours.as_deref());
    let mut fx = RowEffects::default();
    let is_slider = |i: usize| list.get(i).is_some_and(|s| s.slider.is_some());
    let is_list = |i: usize| settings.choices(i).is_some();
    // What the test aids asked to be logged once the page is made whole.
    let (mut log_tab, mut log_select, mut inspect) = (false, None::<String>, false);
    // The settings aid: show the page and flip the switch, as a click on its row, or set
    // the slider (`key=value`).
    let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut aid.switches).into_iter().partition(|(t, _)| *t <= now);
    aid.switches = later;
    for (_, item) in due {
        info!("test aid NVF_PLAY_TEST_SETTINGS: {item}");
        let control = |c: ControlRow| RowId::Control(c);
        if let Some(b) = item.strip_prefix("bind:") {
            // `bind:<control>=<key or button>`: as Enter on its row and that key.
            let parsed = b.split_once('=').and_then(|(c, i)| Some((Bindings::by_name(c)?, crate::bindings::parse_input(i)?)));
            match parsed {
                Some((c, input)) => {
                    show_row(&mut page, &mut view, layout, control(ControlRow::Binding(c, device)));
                    commands.queue(move |world: &mut World| crate::bindings::rebind(world, c, input));
                }
                None => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected bind:<control>=<key or mouse button name>"),
            }
            continue;
        }
        if let Some(b) = item.strip_prefix("padbind:") {
            // `padbind:<control>=<button>`: as A on its row and that button.
            let parsed = b.split_once('=').and_then(|(c, i)| Some((Bindings::by_name(c)?, crate::pad::parse_pad(i)?)));
            match parsed {
                Some((c, p)) => {
                    show_row(&mut page, &mut view, layout, control(ControlRow::Binding(c, device)));
                    commands.queue(move |world: &mut World| crate::bindings::rebind_pad(world, c, p));
                }
                None => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected padbind:<control>=<controller button name>"),
            }
            continue;
        }
        if let Some(v) = item.strip_prefix("rumble:") {
            // `rumble:1` or `rumble:0`: the Rumble row set so.
            match v {
                "1" | "0" => {
                    let on = v == "1";
                    show_row(&mut page, &mut view, layout, control(ControlRow::Rumble));
                    commands.queue(move |world: &mut World| crate::bindings::set_rumble(world, on));
                }
                _ => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected rumble:1 or rumble:0"),
            }
            continue;
        }
        if let Some(v) = item.strip_prefix("invert:") {
            // `invert:1` or `invert:0`: the Invert Y row set so.
            match v {
                "1" | "0" => {
                    let on = v == "1";
                    show_row(&mut page, &mut view, layout, control(ControlRow::Invert));
                    commands.queue(move |world: &mut World| crate::bindings::set_invert_y(world, on));
                }
                _ => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected invert:1 or invert:0"),
            }
            continue;
        }
        if let Some(v) = item.strip_prefix("automatic-input:") {
            match v {
                "0" | "1" => {
                    let on = v == "1";
                    show_row(&mut page, &mut view, layout, control(ControlRow::AutomaticInput));
                    commands.queue(move |world: &mut World| crate::bindings::set_automatic_input_switching(world, on));
                }
                _ => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected automatic-input:0 or 1"),
            }
            continue;
        }
        if let Some(v) = item.strip_prefix("sensitivity:") {
            // `sensitivity:<percent>`: the device in use's sensitivity slider set to it (onto its
            // steps).
            match v.trim_end_matches('%').parse::<f32>() {
                Ok(p) if p.is_finite() => {
                    let m = crate::bindings::step_sensitivity(p / 100.0, 0);
                    show_row(&mut page, &mut view, layout, control(ControlRow::Sensitivity(device)));
                    commands.queue(move |world: &mut World| crate::bindings::set_sensitivity(world, m));
                }
                _ => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected sensitivity:<percent>"),
            }
            continue;
        }
        if let Some(v) = item.strip_prefix("vertical:") {
            // `vertical:<percent>`: the device in use's Vertical Sensitivity set to it (onto
            // the sliders' steps).
            match v.trim_end_matches('%').parse::<f32>() {
                Ok(p) if p.is_finite() => {
                    let m = crate::bindings::step_sensitivity(p / 100.0, 0);
                    show_row(&mut page, &mut view, layout, control(ControlRow::Vertical(device)));
                    commands.queue(move |world: &mut World| crate::bindings::set_vertical(world, m));
                }
                _ => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected vertical:<percent>"),
            }
            continue;
        }
        if let Some(d) = item.strip_prefix("device:") {
            match InputDevice::parse(d) {
                Some(d) => {
                    show_row(&mut page, &mut view, layout, control(ControlRow::Device));
                    commands.queue(move |world: &mut World| crate::bindings::set_device(world, d));
                }
                None => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: expected device:Keyboard and Mouse or device:Controller"),
            }
            continue;
        }
        if let Some(t) = item.strip_prefix("tab:") {
            // `tab:<name>`: the tab shown on its first row (none on an empty one).
            match layout.tab_names().into_iter().find(|n| n.eq_ignore_ascii_case(t.trim())) {
                Some(name) => {
                    show_scope(&mut page, &mut view, SettingsScope::Tab(name));
                    log_tab = true;
                }
                None => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: no such tab; there are {:?}", layout.tab_names()),
            }
            continue;
        }
        if let Some(k) = item.strip_prefix("select:") {
            // `select:<key>`: the row chosen, nothing changed.
            match settings.index(k.trim()) {
                Some(i) => {
                    show_row(&mut page, &mut view, layout, RowId::Setting(i));
                    log_select = Some(k.trim().to_string());
                }
                None => warn!("NVF_PLAY_TEST_SETTINGS: {item:?}: no row {:?}", k.trim()),
            }
            continue;
        }
        if let Some(q) = item.strip_prefix("search:") {
            // `search:<text>`: the search open on that text, the keys on its results.
            let selected = match *page {
                Page::Settings { selected } => selected,
                _ => {
                    show_scope(&mut page, &mut view, SettingsScope::default());
                    None
                }
            };
            open_search(&mut view, selected);
            view.search.query = q.trim().to_string();
            view.focus = SettingsFocus::Rows;
            view.scroll = 0;
            *page = Page::Settings { selected: None };
            continue;
        }
        if item == "changed" {
            // `changed`: the Changed list.
            show_scope(&mut page, &mut view, SettingsScope::Changed);
            log_tab = true;
            continue;
        }
        if item == "layout" {
            log_layout(list, layout);
            continue;
        }
        let (key, value) = match item.split_once('=') {
            Some((k, v)) => (k.trim().to_string(), v.trim().parse::<f32>().ok()),
            None => (item.clone(), None),
        };
        match (settings.index(&key), key.as_str()) {
            (Some(i), _) if is_list(i) => {
                show_row(&mut page, &mut view, layout, RowId::Setting(i));
                match item.split_once('=') {
                    Some((_, v)) => fx.picks.push((i, v.trim().to_string())),
                    None => warn!("NVF_PLAY_TEST_SETTINGS: {key} is a list: give it a choice, {key}=<choice>"),
                }
            }
            (Some(i), _) if is_slider(i) => {
                show_row(&mut page, &mut view, layout, RowId::Setting(i));
                match value {
                    Some(v) => fx.sets.push((i, v)),
                    None => warn!("NVF_PLAY_TEST_SETTINGS: {key} is a slider: give it a value, {key}=0.5"),
                }
            }
            (Some(i), _) => {
                show_row(&mut page, &mut view, layout, RowId::Setting(i));
                fx.flips.push(i);
            }
            (None, _) if settings.alias(&key).is_some() => {
                // An older list's key (settings::set_list_alias): its row shown, its choice taken
                // by the older row's name.
                if let Some(i) = settings.alias(&key).and_then(|(_, a)| a.rows.first()).and_then(|row| settings.index(row)) {
                    show_row(&mut page, &mut view, layout, RowId::Setting(i));
                }
                match item.split_once('=') {
                    Some((_, v)) => {
                        let (key, name) = (key.clone(), v.trim().to_string());
                        commands.queue(move |world: &mut World| {
                            crate::settings::pick_alias(world, &key, &name);
                        });
                    }
                    None => warn!("NVF_PLAY_TEST_SETTINGS: {key} is a list: give it a choice, {key}=<choice>"),
                }
            }
            (None, "page") => show_scope(&mut page, &mut view, SettingsScope::default()),
            (None, "controls") => show_row(&mut page, &mut view, layout, RowId::Control(ControlRow::Device)),
            (None, _) => warn!("NVF_PLAY_TEST_SETTINGS: no switch {key:?}; there are {:?}", list.iter().map(|s| s.key).collect::<Vec<_>>()),
        }
    }
    // The menu aid's actions due now, as the keys they stand for.
    let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut aid.actions).into_iter().partition(|(t, _)| *t <= now);
    aid.actions = later;
    let mut acts: Vec<String> = Vec::new();
    let mut pressed = Pressed { keys: &keys, extra: Vec::new() };
    for (_, a) in due {
        info!("test aid NVF_PLAY_TEST_PAUSE_MENU: {a}");
        if a == "shot" {
            let name = format!("{}pause_{}.png", if under_main { "main_menu_" } else { "" }, match *page {
                Page::Main if view.confirm.is_some() => "quit",
                Page::Main => "main",
                Page::Settings { .. } if view.scope == SettingsScope::Tab(CONTROLS_TAB) => "controls",
                Page::Settings { .. } => "settings",
                Page::Load { .. } => "load",
                Page::Mods => "mods",
            });
            info!("pause menu: screenshot {name}");
            commands.spawn(bevy::render::view::screenshot::Screenshot::primary_window()).observe(bevy::render::view::screenshot::save_to_disk(name));
        }
        // The settings page's verbs press their keys [design: #1953].
        let key = match a.as_str() {
            "heading-down" => Some(KeyCode::PageDown),
            "heading-up" => Some(KeyCode::PageUp),
            "tab-down" => Some(KeyCode::Tab),
            "tab-up" => Some(KeyCode::F1),
            "rail" => Some(KeyCode::F6),
            "left" => Some(KeyCode::ArrowLeft),
            "right" => Some(KeyCode::ArrowRight),
            "inspect" => {
                inspect = true;
                None
            }
            // The search's verbs go in as real input next frame [design: #1953].
            "backspace" | "ctrl-f" | "press" | "release" => {
                if let Some(aid) = input_aid.as_mut() {
                    match a.as_str() {
                        "backspace" => aid.keys.push(aid_key(KeyCode::Backspace, Key::Backspace, None)),
                        "ctrl-f" => aid.keys.extend([aid_key(KeyCode::ControlLeft, Key::Control, None), aid_key(KeyCode::KeyF, Key::Character("f".into()), Some("f"))]),
                        "press" => aid.pointer.push(PointerStep::Press),
                        _ => aid.pointer.push(PointerStep::Release),
                    }
                }
                None
            }
            _ if a.starts_with("type:") || a.starts_with("point:") => {
                if let (Some(aid), Some(t)) = (input_aid.as_mut(), a.strip_prefix("type:")) {
                    aid.keys.extend(typed_keys(t));
                }
                if let Some(t) = a.strip_prefix("point:") {
                    match (input_aid.as_mut(), parse_point(t)) {
                        (Some(aid), Some(at)) => aid.pointer.push(PointerStep::Point(t.trim().to_string(), at)),
                        (_, None) => warn!("NVF_PLAY_TEST_PAUSE_MENU: {a:?}: expected point:search, changed, defaults, back, heading-up, heading-down, tab:<name>, row:<key> or choice:<n>"),
                        _ => {}
                    }
                }
                None
            }
            _ => a.strip_prefix("pad:").and_then(|b| match crate::pad::parse_pad(b) {
                Some(PadInput::Button(button)) => {
                    let k = crate::pad::menu_key_at(button, crate::pad::PAUSE_AT);
                    match k {
                        Some(k) => info!("pause menu: {} presses {k:?}", b.trim()),
                        None => warn!("NVF_PLAY_TEST_PAUSE_MENU: {a:?}: the button presses no key here"),
                    }
                    k
                }
                _ => {
                    warn!("NVF_PLAY_TEST_PAUSE_MENU: {a:?}: expected pad:<controller button name>");
                    None
                }
            }),
        };
        pressed.extra.extend(key);
        acts.push(a);
    }
    let act = |name: &str| acts.iter().any(|a| a == name);
    // A control waiting for its new key takes the next key or mouse button, whatever it
    // is over (with a controller, the next controller button); Esc (Start) cancels.
    if let Some((c, asked_with)) = listening.0 {
        if !matches!(*page, Page::Settings { .. }) {
            listening.0 = None;
        } else {
            let controller = asked_with == InputDevice::Controller;
            let key = keys.get_just_pressed().copied().find(|k| crate::bindings::key_name(*k).is_some() || *k == KeyCode::Escape);
            let start = pads.iter().any(|g| g.just_pressed(GamepadButton::Start));
            if key == Some(KeyCode::Escape) || (controller && start) {
                info!("controls: rebinding cancelled");
                listening.0 = None;
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Back });
                return;
            }
            let pad = crate::pad::PAD_NAMES.iter().map(|(p, _)| *p).filter(|p| !crate::pad::RESERVED_PAD.contains(p)).find(|p| match p {
                PadInput::Button(b) => pads.iter().any(|g| g.just_pressed(*b)),
                PadInput::Stick(d) => sticks.as_ref().is_some_and(|s| s.just_pressed(*d)),
            });
            let input = match key {
                _ if controller => None,
                Some(k) if crate::bindings::RESERVED_KEYS.contains(&k) => None,
                Some(k) => Some(Input::Key(k)),
                None => mouse.get_just_pressed().next().map(|m| Input::Mouse(*m)),
            };
            match (input, pad) {
                (Some(input), _) => {
                    listening.0 = None;
                    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
                    commands.queue(move |world: &mut World| crate::bindings::rebind(world, c, input));
                }
                (None, Some(p)) if controller => {
                    listening.0 = None;
                    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
                    commands.queue(move |world: &mut World| crate::bindings::rebind_pad(world, c, p));
                }
                _ => {}
            }
            return;
        }
    }
    let controls = ControlState {
        device,
        automatic_input: bindings.as_ref().is_some_and(|b| b.automatic_input_switching),
        rumble: bindings.as_ref().is_some_and(|b| b.rumble),
        invert: bindings.as_ref().is_some_and(|b| b.invert_y),
        sensitivity: bindings.as_ref().map_or(1.0, |b| b.sensitivity()),
        vertical: bindings.as_ref().map_or(1.0, |b| b.vertical_sensitivity()),
    };
    // The settings page made whole for what the aids chose ([`sv::normalize`]).
    let mut rows: Vec<ViewEntry> = Vec::new();
    if let Page::Settings { selected } = *page {
        if view.scope == SettingsScope::Search {
            refresh_search(&mut view, layout, list);
        }
        let (scope, sel, e) = sv::normalize(layout, list, view.scope, selected, vis, &view.found);
        view.scope = scope;
        page.set_if_neq(Page::Settings { selected: sel });
        rows = e;
    }
    let mut clicked = None;
    // A list takes the row the pointer moves onto (not one it rests on while the keys
    // move the choice, nor the one a redrawn page puts under it).
    let moved = menu.motion.as_ref().is_some_and(|m| m.delta != Vec2::ZERO);
    let (main_before, confirm_before) = (view.main_selected, view.confirm);
    // The keep question or an open dropdown over the page: the rows under it do not follow
    // the pointer.
    let over = pending.is_some() || view.dropdown.is_some();
    // Each list's first line drawn: the pointer takes only a row in its window, so a row
    // left over from the window before (or one drawn past it) never moves the window or
    // takes a press.
    let load_first = view.scroll.min(menu.list.saves.len().saturating_sub(LOAD_WINDOW.shown));
    let drawn = sv::drawn_slots(&rows, view.scroll.min(rows.len().saturating_sub(lines)), lines);
    let on_settings = matches!(*page, Page::Settings { .. });
    let on_load = matches!(*page, Page::Load { .. });
    let in_window = |b: &PauseButton| match b {
        PauseButton::LoadRow(i) => on_load && LOAD_WINDOW.holds(load_first, *i),
        PauseButton::Row(id) | PauseButton::Meter(id) | PauseButton::SliderStep(id, _) => on_settings && sv::slot_of(&rows, *id).is_some_and(|k| drawn.contains(&k)),
        _ => true,
    };
    for (interaction, b) in &buttons {
        match (interaction, b, &mut *page) {
            (PointerState::Hovered | PointerState::Pressed, b, _) if !in_window(b) => {}
            (PointerState::Hovered, PauseButton::Choice(r, k), _) if moved => {
                if let Some(o) = view.dropdown.as_mut().filter(|o| o.row == *r && (o.first..o.first + DROP_ROWS).contains(k)) {
                    o.at = *k;
                }
            }
            (PointerState::Hovered, PauseButton::Keep(k), _) if moved => view.keep_selected = *k,
            (PointerState::Hovered, ..) if over => {}
            (PointerState::Pressed, PauseButton::Meter(id), Page::Settings { selected }) => {
                *selected = Some(*id);
                view.dragging = Some(*id);
                view.focus = SettingsFocus::Rows;
            }
            (PointerState::Pressed, PauseButton::ScrollTrack, _) => view.bar_dragging = true,
            (PointerState::Pressed, ..) => clicked = Some(*b),
            (PointerState::Hovered, PauseButton::Row(id), Page::Settings { selected }) if moved => {
                *selected = Some(*id);
                view.focus = SettingsFocus::Rows;
            }
            (PointerState::Hovered, PauseButton::LoadRow(i), Page::Load { selected }) if moved => *selected = *i,
            (PointerState::Hovered, PauseButton::Column(i), Page::Main) if moved && view.confirm.is_none() && !under_main => view.main_selected = *i,
            (PointerState::Hovered, PauseButton::Confirm(i), Page::Main) if moved && view.confirm.is_some() => view.confirm = Some(*i),
            _ => {}
        }
    }
    // The rail's line under the pointer: boxed unfilled and bright, the focus sound once
    // as the pointer comes onto it (every button looked at: a redrawn page's are new)
    // [design: #1953; docs/design/screen-menu.md §3.1].
    let hover = if matches!(*page, Page::Settings { .. }) && !over {
        pointed.iter().find_map(|(s, b)| match (s, b) {
            // Only a pointer resting on it: a press keeps its state when dragged away.
            (PointerState::Hovered, PauseButton::Rail(item)) => Some(*item),
            _ => None,
        })
    } else {
        None
    };
    if hover != view.rail_hover {
        if let Some(item) = hover {
            info!("pause menu: hover {}", item.name());
            sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Focus });
        }
        view.rail_hover = hover;
    }
    // "Report a bug" under the pointer: boxed unfilled, the focus sound once.
    let on_report = !over && pointed.iter().any(|(s, b)| *s == PointerState::Hovered && *b == PauseButton::Report);
    if on_report != view.report_hover {
        if on_report {
            sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Focus });
        }
        view.report_hover = on_report;
    }
    if act("defaults") && matches!(*page, Page::Settings { .. }) {
        clicked = Some(PauseButton::Defaults);
    }
    // The keep question takes every key and click until it is answered.
    if pending.is_some() {
        let answer = match clicked {
            Some(PauseButton::Keep(k)) => Some(k),
            _ if pressed.any([KeyCode::Escape, KeyCode::Backspace]) || act("revert") => Some(1),
            _ if act("keep") => Some(0),
            _ if pressed.any([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) => Some(view.keep_selected),
            _ => None,
        };
        let turn = [KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::KeyA, KeyCode::KeyD, KeyCode::KeyW, KeyCode::KeyS, KeyCode::Tab];
        match answer {
            Some(0) => {
                commands.queue(crate::settings::keep);
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Picked });
            }
            Some(_) => {
                commands.queue(crate::settings::revert);
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Back });
            }
            None if pressed.any(turn) => {
                view.keep_selected = 1 - view.keep_selected.min(1);
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Focus });
            }
            None => {}
        }
        if answer.is_some() {
            view.keep_selected = 0;
        }
        return;
    }
    // An open dropdown takes the keys and clicks until it closes.
    if let (Some(mut o), Page::Settings { .. }) = (view.dropdown, *page) {
        let names = view.drawn.as_ref().and_then(|d| d.list(o.row)).map(|(n, _)| n.to_vec()).unwrap_or_default();
        let n = names.len();
        let before = o.at;
        let mut take = None;
        let mut shut = false;
        match clicked {
            Some(PauseButton::Choice(r, k)) if r == o.row => take = Some(k),
            Some(_) => shut = true,
            None if mouse.just_pressed(MouseButton::Left) => shut = true,
            None if pressed.any([KeyCode::Escape, KeyCode::Backspace]) || act("back") => shut = true,
            None if pressed.any([KeyCode::ArrowUp, KeyCode::KeyW]) || act("up") => o.at = o.at.saturating_sub(1),
            None if pressed.any([KeyCode::ArrowDown, KeyCode::KeyS]) || act("down") => o.at = (o.at + 1).min(n.saturating_sub(1)),
            None if pressed.any([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) || act("pick") => take = Some(o.at),
            None => {}
        }
        let last = n.saturating_sub(DROP_ROWS);
        let wheel = menu.scroll.as_ref().map_or(0.0, |s| s.delta.y);
        if wheel > 0.0 {
            o.first = o.first.saturating_sub(1);
        } else if wheel < 0.0 {
            o.first = (o.first + 1).min(last);
        }
        if o.at != before {
            o.first = o.first.max((o.at + 1).saturating_sub(DROP_ROWS)).min(o.at).min(last);
            sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Focus });
        }
        let out_of_reach = take.is_some_and(|k| view.drawn.as_ref().is_some_and(|d| d.choice_reason(o.row, k).is_some()));
        if let Some((k, name)) = take.and_then(|k| Some((k, names.get(k)?))).filter(|_| out_of_reach) {
            // Out of reach: the list stays open on it, its reason under the rows.
            refuse_choice(&mut commands, o.row, name.clone());
            o.at = k;
        } else if let Some(name) = take.and_then(|k| names.get(k)) {
            pick_choice(&mut commands, &mut sounds, o.row, name.clone());
            shut = true;
        } else if shut {
            sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Back });
        }
        view.dropdown = (!shut).then_some(o);
        return;
    }
    view.dropdown = None;
    // "Report a bug": the form opens over the page (nvforever's `reports/form.rs`).
    if clicked == Some(PauseButton::Report) {
        if let Some(r) = report.as_mut().filter(|r| r.shown) {
            info!("pause menu: report a bug");
            r.asked = true;
            sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Picked });
        }
        return;
    }
    // A click on the column's entry of another page (the page's own entry closes it). From
    // an open search the search closes first, and the frame's keys (its text) reach no
    // other page [design: #1953].
    // Only a click that leaves the page closes it: on the main menu, an entry the column
    // does nothing with (New Game, a greyed one) leaves the search as it is.
    let search_open = matches!(*page, Page::Settings { .. }) && view.search.from.is_some();
    let column_exit = match clicked {
        Some(PauseButton::Column(_)) if !under_main => *page != Page::Main,
        Some(PauseButton::Column(i)) => main_menu.as_ref().and_then(|m| m.column().get(i).map(|c| c.0)).is_some_and(|l| page.opened_by(l) || l.starts_with("Load") || l.starts_with("Settings")),
        _ => false,
    };
    let mut swallow = false;
    if search_open && column_exit {
        let _ = leave_search(&mut view, &mut sounds);
        swallow = true;
    }
    match clicked {
        Some(PauseButton::Column(i)) if !under_main && *page != Page::Main => {
            let closes = entries.get(i).is_some_and(|e| page.opened_by(e.label()));
            *page = Page::Main;
            view.main_selected = i;
            if closes {
                info!("pause menu: {} again, back to the column", entries[i].label());
                clicked = None;
            } else if entries.get(i).is_some_and(|e| matches!(e, Entry::Load | Entry::Settings | Entry::Mods)) {
                clicked = Some(PauseButton::Column(i));
            } else {
                clicked = None;
            }
        }
        Some(PauseButton::Column(i)) if under_main => {
            let label = main_menu.as_ref().and_then(|m| m.column().get(i).map(|c| c.0));
            match label {
                Some(l) if page.opened_by(l) => {
                    info!("pause menu: {l} again, back to the main menu");
                    *page = Page::Main;
                }
                Some(l) if l.starts_with("Load") => *page = Page::Load { selected: 0 },
                Some(l) if l.starts_with("Settings") => {
                    open_settings(&mut page, &mut view, kept_tab);
                    log_open = true;
                }
                _ => {}
            }
            if matches!(*page, Page::Load { .. }) {
                menu.list.open(menu.cfg.dir.as_deref());
            }
            clicked = None;
        }
        Some(PauseButton::Defaults) if matches!(*page, Page::Settings { .. }) => defaults(&view, layout, list, &mut commands, &mut sounds),
        _ => {}
    }
    let no_keys = ButtonInput::<KeyCode>::default();
    let keys: &ButtonInput<KeyCode> = if swallow {
        // The systems after this one do not see them either.
        commands.queue(|world: &mut World| {
            if let Some(mut k) = world.get_resource_mut::<ButtonInput<KeyCode>>() {
                k.clear();
            }
        });
        // The aids' keys and the row actions read through `pressed` too.
        pressed.keys = &no_keys;
        pressed.extra.clear();
        &no_keys
    } else {
        &keys
    };
    match &mut *page {
        Page::Main if view.confirm.is_some() => {
            let sel = view.confirm.unwrap_or(2);
            let mut take = None;
            if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || act("back") {
                take = Some(2);
            } else if let Some(PauseButton::Confirm(i)) = clicked {
                take = Some(i);
            } else if matches!(clicked, Some(PauseButton::Column(i)) if entries.get(i) == Some(&Entry::Quit)) {
                // Quit again: as Cancel.
                take = Some(2);
            } else if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) || act("up") {
                view.confirm = Some(step_row(sel, -1, CONFIRM.len()));
            } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) || act("down") {
                view.confirm = Some(step_row(sel, 1, CONFIRM.len()));
            } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) || act("pick") {
                take = Some(sel);
            }
            match take {
                Some(0) => {
                    info!("pause menu: quit to the main menu");
                    commands.queue(reset_resume);
                    to_main.write(MainMenuRequest);
                }
                Some(1) => {
                    info!("pause menu: quit");
                    commands.queue(reset_resume);
                    exit.write(AppExit::Success);
                }
                Some(_) => {
                    info!("pause menu: quit cancelled");
                    view.confirm = None;
                    sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Back });
                }
                None => {}
            }
            if take.is_some_and(|t| t < 2) {
                sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Picked });
            }
        }
        Page::Main => {
            let n = entries.len();
            let mut take = match clicked {
                Some(PauseButton::Column(i)) => entries.get(i).copied(),
                _ => None,
            };
            if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) {
                view.main_selected = step_row(view.main_selected, -1, n);
            } else if keys.any_just_pressed([KeyCode::ArrowDown]) {
                view.main_selected = step_row(view.main_selected, 1, n);
            } else if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
                take = entries.get(view.main_selected).copied();
            }
            if keys.just_pressed(KeyCode::KeyQ) || act("quit") {
                take = Some(Entry::Quit);
            } else if keys.just_pressed(KeyCode::KeyN) || act("save") {
                take = Some(Entry::Save);
            } else if keys.just_pressed(KeyCode::KeyL) || act("load") {
                take = Some(Entry::Load);
            } else if keys.just_pressed(KeyCode::KeyS) || act("settings") {
                take = Some(Entry::Settings);
            } else if act("mods") && mods {
                take = Some(Entry::Mods);
            } else if keys.just_pressed(KeyCode::Escape) {
                take = Some(Entry::Continue);
            }
            if let Some(e) = take {
                view.main_selected = entries.iter().position(|x| *x == e).unwrap_or(0);
                if e != Entry::Continue {
                    sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Picked });
                }
            }
            // Any other choice while a resume waits ends the wait; the graphics change stays
            // latched, so the next Continue waits again [design: graphics_resume.rs].
            if take.is_some_and(|e| e != Entry::Continue) && resume.pending() {
                commands.queue(cancel_resume);
            }
            match take {
                Some(Entry::Quit) => {
                    info!("pause menu: quit?");
                    view.confirm = Some(2);
                }
                Some(Entry::Save) => {
                    let name = new_save_name(&menu.cfg);
                    info!("pause menu: saving as {name:?}");
                    match menu.cfg.path_of(&name) {
                        Ok(path) => {
                            menu.status.waiting_for = Some(path);
                            menu.status.line = format!("Saving \"{name}\"...");
                            menu.saves.write(crate::save::SaveRequest { name, kind: SaveKind::Manual });
                        }
                        Err(e) => menu.status.line = format!("Cannot save: {e}"),
                    }
                }
                Some(Entry::Load) => {
                    info!("pause menu: load page");
                    menu.list.open(menu.cfg.dir.as_deref());
                    *page = Page::Load { selected: 0 };
                }
                Some(Entry::Settings) => {
                    info!("pause menu: settings");
                    open_settings(&mut page, &mut view, kept_tab);
                    log_open = true;
                }
                Some(Entry::Mods) => {
                    info!("pause menu: mods page");
                    if let Some(m) = mods_page.as_deref_mut() {
                        *m = Default::default();
                    }
                    *page = Page::Mods;
                }
                Some(Entry::Continue) if !resume.pending() => {
                    commands.queue(request_resume);
                }
                // Continue while a resume is already pending does nothing, as no choice does.
                Some(Entry::Continue) | None => {}
            }
        }
        Page::Settings { selected } => {
            let tabs = layout.tab_names();
            // The tab `step` from what is listed, round the ends (from a search or the
            // Changed list: the first or the last).
            let tab_from = |scope: SettingsScope, step: i32| -> &'static str {
                let n = tabs.len() as i32;
                match tabs.iter().position(|t| SettingsScope::Tab(t) == scope) {
                    Some(at) => tabs[(at as i32 + step).rem_euclid(n) as usize],
                    None if step < 0 => tabs[tabs.len() - 1],
                    None => tabs[0],
                }
            };
            let mut sel = *selected;
            let mut leave = false;
            let mut to_scope: Option<SettingsScope> = None;
            let mut heading: Option<i32> = None;
            let mut row_step: Option<i32> = None;
            let mut row_act: Option<(RowId, RowAction)> = None;
            let mut rail_take: Option<RailItem> = None;
            let in_search = view.scope == SettingsScope::Search;
            let controller = device == InputDevice::Controller;
            // On the search the letters are its text: W/S, A/D and Space are no keys of
            // the page there [design: #1953].
            let (up, down): (&[KeyCode], &[KeyCode]) = if in_search { (&[KeyCode::ArrowUp], &[KeyCode::ArrowDown]) } else { (&[KeyCode::ArrowUp, KeyCode::KeyW], &[KeyCode::ArrowDown, KeyCode::KeyS]) };
            let take_keys: &[KeyCode] = if in_search { &[KeyCode::Enter, KeyCode::NumpadEnter] } else { &[KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space] };
            let find = (pressed.just(KeyCode::KeyF) && pressed.ctrl()) || act("search");
            // The search has the keys it is given first (module notes): what is typed, then
            // Esc (which goes back to what was listed, and goes no further), then in its
            // text Down or Enter to its results; every other key in its text is its own.
            let typed = typed_in.as_ref().map_or(&[][..], |k| k.0.as_slice());
            let mut handled = false;
            // The frame's typing, in its order: text and Backspace into the text, Enter from
            // the text to the results, Esc closes (what came after it is dropped). An Enter
            // alone on the results is the results' own (it acts on the row).
            let typing = view.focus == SettingsFocus::SearchText;
            if in_search && clicked.is_none() && (typing || typed.iter().any(|k| *k != SearchKey::Enter)) && !typed.is_empty() {
                let mut edited = false;
                for k in typed {
                    match k {
                        SearchKey::Text(t) => {
                            view.search.query.push_str(t);
                            view.focus = SettingsFocus::SearchText;
                            edited = true;
                        }
                        SearchKey::Backspace => {
                            view.search.query.pop();
                            view.focus = SettingsFocus::SearchText;
                            edited = true;
                        }
                        SearchKey::Enter => {
                            if view.focus == SettingsFocus::SearchText {
                                view.focus = SettingsFocus::Rows;
                                info!("pause menu: the keys on the results");
                                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Focus });
                            }
                        }
                        SearchKey::Escape => {
                            sel = leave_search(&mut view, &mut sounds);
                            log_tab = true;
                            edited = false;
                            break;
                        }
                    }
                }
                if edited {
                    view.scroll = 0;
                    sel = None;
                }
                handled = true;
                // Then the commands that come as keys without text, after it: a controller's
                // B (Esc) and A (Enter) and D-pad Down, the aids' `back`, `pick` and `down`.
                // A key the window typed was taken in its place above.
                let typed_has = |k: SearchKey| typed.contains(&k);
                if view.scope == SettingsScope::Search {
                    if (pressed.just(KeyCode::Escape) && !typed_has(SearchKey::Escape)) || act("back") {
                        sel = leave_search(&mut view, &mut sounds);
                        log_tab = true;
                    } else if view.focus == SettingsFocus::SearchText
                        && ((pressed.any([KeyCode::Enter, KeyCode::NumpadEnter]) && !typed_has(SearchKey::Enter)) || pressed.just(KeyCode::ArrowDown) || act("down") || act("pick"))
                    {
                        view.focus = SettingsFocus::Rows;
                        info!("pause menu: the keys on the results");
                        sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Focus });
                    }
                }
            } else if in_search && (pressed.just(KeyCode::Escape) || act("back") || clicked == Some(PauseButton::Back)) {
                sel = leave_search(&mut view, &mut sounds);
                log_tab = true;
                handled = true;
            } else if in_search && clicked.is_none() && view.focus == SettingsFocus::SearchText {
                if pressed.any([KeyCode::ArrowDown, KeyCode::Enter, KeyCode::NumpadEnter]) || act("down") || act("pick") {
                    view.focus = SettingsFocus::Rows;
                    info!("pause menu: the keys on the results");
                    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Focus });
                }
                handled = true;
            } else if find && clicked.is_none() {
                open_search(&mut view, sel);
                sel = None;
                sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Focus });
                handled = true;
            }
            let back = !in_search && (pressed.any([KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::Back) || act("back"));
            if !handled { match clicked {
                _ if back => leave = true,
                Some(PauseButton::Row(id)) => {
                    sel = Some(id);
                    view.focus = SettingsFocus::Rows;
                    row_act = Some((id, RowAction::Activate));
                }
                Some(PauseButton::SliderStep(id, step)) => {
                    sel = Some(id);
                    view.focus = SettingsFocus::Rows;
                    row_act = Some((id, RowAction::Step(step)));
                }
                Some(PauseButton::Rail(item)) => rail_take = Some(item),
                Some(PauseButton::SearchField) => rail_take = Some(RailItem::Search),
                Some(PauseButton::HeadingStep(step)) => {
                    view.focus = SettingsFocus::Rows;
                    heading = Some(step);
                }
                _ if pressed.just(KeyCode::Tab) && !pressed.shift() => to_scope = Some(SettingsScope::Tab(tab_from(view.scope, 1))),
                _ if pressed.just(KeyCode::Tab) || pressed.just(KeyCode::F1) => to_scope = Some(SettingsScope::Tab(tab_from(view.scope, -1))),
                _ if pressed.just(KeyCode::F6) => {
                    view.focus = match view.focus {
                        SettingsFocus::Rows | SettingsFocus::SearchText => SettingsFocus::Rail(RailItem::of(view.scope)),
                        SettingsFocus::Rail(_) => SettingsFocus::Rows,
                    };
                    match view.focus {
                        SettingsFocus::Rail(r) => info!("pause menu: the keys on the rail ({})", r.name()),
                        SettingsFocus::Rows | SettingsFocus::SearchText => info!("pause menu: the keys on the rows"),
                    }
                    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Focus });
                }
                _ => match view.focus {
                    SettingsFocus::Rail(item) => {
                        // A controller's keys pass over Search: it is typed [design: #1953].
                        let items: Vec<RailItem> = sv::rail(layout).into_iter().filter(|r| !(controller && *r == RailItem::Search)).collect();
                        let at = items.iter().position(|r| *r == item).unwrap_or(0);
                        let step = if pressed.any(up.iter().copied()) || act("up") {
                            -1
                        } else if pressed.any(down.iter().copied()) || act("down") {
                            1
                        } else {
                            0
                        };
                        if step != 0 {
                            let to = items[step_row(at, step, items.len())];
                            view.focus = SettingsFocus::Rail(to);
                            info!("pause menu: rail {}", to.name());
                            sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Focus });
                        } else if pressed.any(take_keys.iter().copied()) || act("pick") {
                            rail_take = Some(item);
                        }
                    }
                    SettingsFocus::SearchText => {}
                    SettingsFocus::Rows => {
                        let letters = !in_search && sel.is_some_and(|id| takes_letters(&settings, id));
                        if pressed.any(up.iter().copied()) || act("up") {
                            row_step = Some(-1);
                        } else if pressed.any(down.iter().copied()) || act("down") {
                            row_step = Some(1);
                        } else if pressed.just(KeyCode::PageUp) {
                            heading = Some(-1);
                        } else if pressed.just(KeyCode::PageDown) {
                            heading = Some(1);
                        } else if pressed.any(take_keys.iter().copied()) || act("pick") {
                            row_act = sel.map(|id| (id, RowAction::Activate));
                        } else if pressed.just(KeyCode::ArrowLeft) || (letters && pressed.just(KeyCode::KeyA)) {
                            row_act = sel.map(|id| (id, RowAction::Step(-1)));
                        } else if pressed.just(KeyCode::ArrowRight) || (letters && pressed.just(KeyCode::KeyD)) {
                            row_act = sel.map(|id| (id, RowAction::Step(1)));
                        } else if pressed.just(KeyCode::Delete) {
                            row_act = sel.map(|id| (id, RowAction::Clear));
                        }
                    }
                },
            } }
            match rail_take {
                Some(RailItem::Tab(t)) => to_scope = Some(SettingsScope::Tab(t)),
                Some(RailItem::Search) => {
                    open_search(&mut view, sel);
                    sel = None;
                    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Picked });
                }
                Some(RailItem::Changed) => to_scope = Some(SettingsScope::Changed),
                Some(RailItem::Defaults) => defaults(&view, layout, list, &mut commands, &mut sounds),
                // Back with the search open closes it, as Esc; only then does it leave.
                Some(RailItem::Back) if view.scope == SettingsScope::Search => {
                    sel = leave_search(&mut view, &mut sounds);
                    log_tab = true;
                }
                Some(RailItem::Back) => leave = true,
                None => {}
            }
            if leave {
                *page = Page::Main;
            } else {
                if let Some(scope) = to_scope {
                    let on_rail = matches!(view.focus, SettingsFocus::Rail(_));
                    if scope != view.scope {
                        view.scroll = 0;
                    }
                    if scope != SettingsScope::Search {
                        close_search(&mut view);
                    }
                    view.scope = scope;
                    sel = None;
                    rows = sv::project(layout, list, scope, vis, &view.found);
                    // On the rail the keys stay there, on the tab's line; a tab taken with
                    // rows to choose gives them to the rows (a click, Enter on the rail).
                    view.focus = if on_rail && (rail_take.is_none() || rows.is_empty()) { SettingsFocus::Rail(RailItem::of(scope)) } else { SettingsFocus::Rows };
                    log_tab = true;
                    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Picked });
                }
                if let Some(step) = row_step {
                    sel = sv::step_selectable(&rows, sel, step);
                }
                if let Some(step) = heading {
                    if let Some((id, p)) = sv::step_heading(&rows, sel, step) {
                        sel = Some(id);
                        // A jump always shows its heading, even onto the row already chosen
                        // (a single section wraps to its own first row) [design: #1953].
                        view.followed = None;
                        info!("pause menu: heading {}; selected {}", p.map_or_else(|| view.scope.name().to_string(), |p| p.path()), id.name(list));
                    }
                }
                if let Some((id, a)) = row_act {
                    act_on_row(id, a, &settings, &mut view, &mut fx, controls, &mut listening, &mut commands, &mut sounds);
                }
                *page = Page::Settings { selected: sel };
            }
            // A drag on a meter: the value under the pointer while the button is held
            // (the page is drawn again as the value changes, so the meter is looked up
            // by its row each frame).
            if let Some(id) = view.dragging {
                if mouse.pressed(MouseButton::Left) {
                    let under = meters.iter().find(|(b, _)| **b == PauseButton::Meter(id)).and_then(|(_, at)| at.normalized);
                    match (id, under) {
                        (RowId::Setting(i), Some(p)) => {
                            let v = (p.x + 0.5).clamp(0.0, 1.0);
                            if list.get(i).and_then(|s| s.slider).is_some_and(|s| s.fit(v) != s.fit(view_value(&view, i))) {
                                fx.sets.push((i, v));
                            }
                        }
                        (RowId::Control(ControlRow::Sensitivity(_)), Some(p)) => {
                            let m = crate::bindings::sensitivity_at_part(p.x + 0.5);
                            if (m - controls.sensitivity).abs() > 1e-4 {
                                commands.queue(move |world: &mut World| crate::bindings::set_sensitivity(world, m));
                            }
                        }
                        (RowId::Control(ControlRow::Vertical(_)), Some(p)) => {
                            let m = crate::bindings::sensitivity_at_part(p.x + 0.5);
                            if (m - controls.vertical).abs() > 1e-4 {
                                commands.queue(move |world: &mut World| crate::bindings::set_vertical(world, m));
                            }
                        }
                        _ => {}
                    }
                } else {
                    view.dragging = None;
                }
            }
        }
        // The mod's page takes the pointer and the keys through its tiles
        // (`crate::tiles::input`); Esc, Backspace or Back go back to the column.
        // MCM's window closing itself (its Back) closes the page too (`crate::tiles::mods`).
        Page::Mods => {
            let by_mod = mods_page.as_deref().is_some_and(|m| m.closed);
            if by_mod || keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::Back) || act("back") {
                info!("pause menu: mods page closed");
                *page = Page::Main;
            }
        }
        Page::Load { selected } if view.delete_ask.is_some() => {
            // The delete question has the keys (module notes).
            let yes = keys.any_just_pressed([KeyCode::KeyY, KeyCode::Enter, KeyCode::NumpadEnter]) || clicked == Some(PauseButton::DeleteAnswer(true)) || act("yes");
            let no = keys.any_just_pressed([KeyCode::KeyN, KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::DeleteAnswer(false)) || act("no") || act("back");
            if yes {
                let row = view.delete_ask.take().unwrap_or(*selected);
                if let Some((path, _)) = menu.list.saves.get(row).cloned() {
                    match delete_save(&path) {
                        Ok(()) => info!("pause menu: deleted {}", path.display()),
                        Err(e) => warn!("pause menu: could not delete {}: {e}", path.display()),
                    }
                    menu.list.open(menu.cfg.dir.as_deref());
                    *selected = (*selected).min(menu.list.saves.len().saturating_sub(1));
                }
            } else if no {
                view.delete_ask = None;
                info!("pause menu: delete cancelled");
            }
        }
        Page::Load { selected } => {
            let rows = menu.list.saves.len();
            let mut pick = None;
            if (keys.just_pressed(KeyCode::Delete) || clicked == Some(PauseButton::Delete) || act("delete")) && *selected < rows {
                view.delete_ask = Some(*selected);
                info!("pause menu: delete {}?", menu.list.saves.get(*selected).map(|(p, _)| stem(p)).unwrap_or_default());
            } else if keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace]) || clicked == Some(PauseButton::Back) || act("back") {
                *page = Page::Main;
            } else if let Some(PauseButton::LoadRow(i)) = clicked {
                *selected = i;
                pick = Some(i);
            } else if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) || act("up") {
                *selected = (*selected).min(rows.saturating_sub(1)).saturating_sub(1);
            } else if keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) || act("down") {
                *selected = (*selected + 1).min(rows.saturating_sub(1));
            } else if (keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) || act("pick")) && *selected < rows {
                pick = Some(*selected);
            }
            if let Some((path, _)) = pick.and_then(|i| menu.list.saves.get(i)) {
                info!("pause menu: load {}", path.display());
                commands.queue(reset_resume);
                menu.loads.write(crate::save::LoadRequest { path: path.clone() });
            }
        }
    }
    // The settings page made whole again after the keys (a new tab, a row of another).
    if let Page::Settings { selected } = *page {
        if view.scope == SettingsScope::Search {
            refresh_search(&mut view, layout, list);
        }
        let (scope, sel, e) = sv::normalize(layout, list, view.scope, selected, vis, &view.found);
        view.scope = scope;
        page.set_if_neq(Page::Settings { selected: sel });
        rows = e;
    }
    // The settings page's and the load page's window of rows: the wheel, the scroll bar's
    // arrows and its marker move it, and the choice with it where it would leave the
    // window; else the window follows the choice (the keys'; the pointer takes only rows in
    // the window, so it never moves it).
    let window = match *page {
        Page::Settings { .. } => Some((rows.len(), settings_window(lines))),
        Page::Load { .. } if view.delete_ask.is_none() => Some((menu.list.saves.len(), LOAD_WINDOW)),
        Page::Load { selected } => {
            view.scroll = crate::menu::model::keep_in_view(view.scroll, selected, menu.list.saves.len(), LOAD_ROWS);
            None
        }
        Page::Main | Page::Mods => None,
    };
    if let Some((n, w)) = window.filter(|w| w.0 > 0) {
        use bevy::input::mouse::MouseScrollUnit;
        view.wheel += menu.scroll.as_ref().map_or(0.0, |s| match s.unit {
            MouseScrollUnit::Line => s.delta.y,
            MouseScrollUnit::Pixel => s.delta.y / WHEEL_PIXELS,
        });
        let notches = view.wheel.trunc();
        view.wheel -= notches;
        // The wheel turned down shows the rows further down.
        let mut by = -(notches as i64) + i64::from(act("wheel-down")) - i64::from(act("wheel-up"));
        if let Some(PauseButton::ScrollStep(s)) = clicked {
            by += i64::from(s);
        }
        let last = n.saturating_sub(w.shown);
        let mut first = view.scroll.min(last);
        let mut scrolled = by != 0;
        if view.bar_dragging {
            if mouse.pressed(MouseButton::Left) {
                if let Some(p) = meters.iter().find(|(b, _)| **b == PauseButton::ScrollTrack).and_then(|(_, at)| at.normalized) {
                    first = w.first_at(n, p.y + 0.5);
                    scrolled = true;
                }
            } else {
                view.bar_dragging = false;
            }
        }
        if scrolled {
            first = (first as i64 + by).clamp(0, last as i64) as usize;
        }
        match &mut *page {
            Page::Settings { selected } if scrolled => {
                first = sv::land(&rows, first, by, w.shown);
                *selected = sv::clamp_into_window(&rows, first, *selected, w.shown);
                view.followed = *selected;
            }
            Page::Settings { selected } => {
                first = sv::window_first(&rows, first, *selected, view.followed, w.shown);
                view.followed = *selected;
            }
            Page::Load { selected } if scrolled => *selected = (*selected).clamp(first, (first + w.shown).min(n) - 1),
            Page::Load { selected } => first = crate::menu::model::keep_in_view(first, *selected, n, w.shown),
            Page::Main | Page::Mods => {}
        }
        if first != view.scroll {
            info!("pause menu: rows {}-{} of {} shown", first + 1, (first + w.shown).min(n), n);
            view.scroll = first;
        }
    }
    if let Page::Settings { selected } = *page {
        remember(&mut view, selected);
        let tab = match view.last {
            Some((SettingsScope::Tab(t), ..)) => Some(t),
            _ => None,
        };
        if let (Some(t), Some(store)) = (tab, tab_store.as_mut()) {
            store.keep(t);
        }
        if log_open {
            info!("pause menu: settings open on {}; selected {}", view.scope.name(), selected.map_or_else(|| "none".to_string(), |id| id.name(list)));
        }
        if log_tab {
            let what = if matches!(view.scope, SettingsScope::Tab(_)) { "tab" } else { "list" };
            info!("pause menu: {what} {}; selected {}", view.scope.name(), selected.map_or_else(|| "none".to_string(), |id| id.name(list)));
        }
        if let Some(k) = log_select {
            info!("pause menu: select {k}: {} on {}", selected.map_or_else(|| "none".to_string(), |id| id.name(list)), view.scope.name());
        }
        // The search's text changed: what it finds now (the rows as listed).
        if view.scope == SettingsScope::Search && view.search.logged.as_deref() != Some(view.search.query.as_str()) {
            let q = view.search.query.clone();
            if !q.is_empty() {
                let keys: Vec<String> = rows
                    .iter()
                    .filter_map(|e| match e {
                        ViewEntry::Row(id) => Some(id.name(list)),
                        ViewEntry::Heading(_) => None,
                    })
                    .collect();
                info!("pause menu: search \"{q}\": {} result(s); keys {}", keys.len(), keys.join(","));
            }
            view.search.logged = Some(q);
        }
        if inspect {
            log_inspect(list, &view, selected, &rows, &sv::rail(layout), device == InputDevice::Controller);
        }
    }
    if let Page::Load { selected } = *page {
        let SaveMenu { list, images, .. } = &mut menu;
        list.ensure_thumb(selected, images);
    }
    // The pointer or the keys moved the first page's choice or the question's: its sound.
    if view.main_selected != main_before && *page == Page::Main || (view.confirm.is_some() && confirm_before.is_some() && view.confirm != confirm_before) {
        sounds.write(UiSound { menu: UiMenu::Pause, event: UiEvent::Focus });
    }
    for i in fx.flips {
        if view_disabled(&view, i) { continue; }
        sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
        commands.queue(move |world: &mut World| {
            crate::settings::flip(world, i);
        });
    }
    for (i, n) in fx.steps {
        if view_disabled(&view, i) { continue; }
        sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
        commands.queue(move |world: &mut World| {
            crate::settings::step_slider(world, i, n);
        });
    }
    for (i, v) in fx.sets {
        if view_disabled(&view, i) { continue; }
        commands.queue(move |world: &mut World| {
            crate::settings::set_slider(world, i, v);
        });
    }
    for (i, name) in fx.picks {
        if view_disabled(&view, i) { continue; }
        if view_out_of_reach(&view, i, &name) {
            refuse_choice(&mut commands, i, name);
            continue;
        }
        pick_choice(&mut commands, &mut sounds, i, name);
    }
}

/// A choice out of reach asked for: nothing taken, `crate::settings::pick` logs why.
fn refuse_choice(commands: &mut Commands, i: usize, name: String) {
    commands.queue(move |world: &mut World| {
        crate::settings::pick(world, i, &name);
    });
}

/// Whether choice `name` of list row `i` was out of reach when the page was last drawn.
fn view_out_of_reach(view: &PauseView, i: usize, name: &str) -> bool {
    let Some(d) = view.drawn.as_ref() else { return false };
    let Some((names, _)) = d.list(i) else { return false };
    names.iter().position(|n| n == name).is_some_and(|k| d.choice_reason(i, k).is_some())
}

/// Take choice `name` of list row `i` (`crate::settings::pick`).
fn pick_choice(commands: &mut Commands, sounds: &mut MessageWriter<crate::ui_sounds::UiSound>, i: usize, name: String) {
    use crate::ui_sounds::{UiEvent, UiMenu, UiSound};
    sounds.write(UiSound { menu: UiMenu::Settings, event: UiEvent::Flipped });
    commands.queue(move |world: &mut World| {
        crate::settings::pick(world, i, &name);
    });
}

/// Open list row `i`'s dropdown on its chosen one.
fn open_dropdown(view: &mut PauseView, i: usize) {
    if view_disabled(view, i) { return; }
    let Some((names, chosen)) = view.drawn.as_ref().and_then(|d| d.list(i)) else { return };
    let at = names.iter().position(|n| n == chosen).unwrap_or(0);
    let first = (at + 1).saturating_sub(DROP_ROWS).min(names.len().saturating_sub(DROP_ROWS));
    info!("pause menu: the list of {} choice(s) open", names.len());
    view.dropdown = Some(Dropdown { row: i, at, first });
}

/// The choice `step` (-1 or 1) from list row `i`'s chosen one, not past the ends, passing
/// over those out of reach.
fn neighbour(view: &PauseView, i: usize, step: i32) -> Option<String> {
    if view_disabled(view, i) { return None; }
    let d = view.drawn.as_ref()?;
    let (names, chosen) = d.list(i)?;
    let mut at = names.iter().position(|n| n == chosen)? as i32;
    loop {
        at += step;
        let k = usize::try_from(at).ok()?;
        let name = names.get(k)?;
        if d.choice_reason(i, k).is_none() {
            return Some(name.clone());
        }
    }
}

/// Whether row `i` was disabled when the page was last drawn.
fn view_disabled(view: &PauseView, i: usize) -> bool {
    view.drawn.as_ref().and_then(|d| d.disabled.get(i)).is_some_and(Option::is_some)
}

/// Row `i`'s value as the page last drew it.
fn view_value(view: &PauseView, i: usize) -> f32 {
    view.drawn.as_ref().and_then(|d| d.states.get(i).copied()).unwrap_or(0.0)
}

/// The main page's status line: the save asked from this menu, once written.
fn show_status(mut done: MessageReader<crate::save::SaveDone>, mut status: ResMut<PauseStatus>) {
    for d in done.read() {
        if status.waiting_for.as_ref() != Some(&d.path) {
            continue;
        }
        status.waiting_for = None;
        status.line = match &d.result {
            Ok(()) => format!("Saved \"{}\"", stem(&d.path)),
            Err(e) => format!("Save failed: {e}"),
        };
    }
}

/// The settings page made whole for the state the frame's changes left (a row hidden or
/// shown since the keys moved): what it lists, its choice, its window, an open dropdown or
/// drag on a row no longer listed closed; and the lines it lists and its rail.
fn settings_now(world: &mut World, list: &[Setting], selected: Option<RowId>) -> (Option<RowId>, Vec<ViewEntry>, Vec<RailItem>) {
    use crate::settings_view as sv;
    let vis = Visibility::of_world(world);
    if !world.contains_resource::<SettingsLayout>() {
        world.init_resource::<SettingsLayout>();
    }
    let (scope, sel, entries, rail) = world.resource_scope(|world, mut layout: Mut<SettingsLayout>| {
        let l = layout.refresh(list);
        let mut view = world.resource_mut::<PauseView>();
        if view.scope == SettingsScope::Search {
            refresh_search(&mut view, l, list);
        }
        let (scope, sel, entries) = sv::normalize(l, list, view.scope, selected, vis, &view.found);
        // A Changed row put back leaves the list: the choice goes to the row after it.
        let before = view.drawn.as_ref().filter(|d| d.scope == SettingsScope::Changed && scope == SettingsScope::Changed).map(|d| d.entries.clone()).unwrap_or_default();
        let sel = sv::reselect(&before, &entries, selected).filter(|id| sv::slot_of(&entries, *id).is_some()).or(sel);
        (scope, sel, entries, sv::rail(l))
    });
    let mut view = world.resource_mut::<PauseView>();
    view.scope = scope;
    // The Changed list: logged as its rows change [design: #1953].
    if scope == SettingsScope::Changed {
        if view.changed_logged.as_ref() != Some(&view.found.changed) {
            let keys: Vec<String> = view.found.changed.iter().map(|id| id.name(list)).collect();
            info!("pause menu: changed {} row(s); keys {}", keys.len(), keys.join(","));
            view.changed_logged = Some(view.found.changed.clone());
        }
    } else {
        view.changed_logged = None;
    }
    view.set_lines();
    let lines = view.lines();
    let at = sel.and_then(|id| sv::slot_of(&entries, id));
    let first = view.scroll.min(entries.len().saturating_sub(lines));
    view.scroll = if at.is_some_and(|k| !sv::drawn_slots(&entries, first, lines).contains(&k)) { sv::follow(&entries, first, at, lines) } else { first };
    if view.dropdown.is_some_and(|o| sv::slot_of(&entries, RowId::Setting(o.row)).is_none()) {
        view.dropdown = None;
    }
    if view.dragging.is_some_and(|id| sv::slot_of(&entries, id).is_none()) {
        view.dragging = None;
    }
    // The colours' sliders: logged as they show or hide [design: #1953].
    if view.colours_shown != Some(vis.colours) {
        let says = |on: bool| if on { "shown" } else { "hidden" };
        info!("pause menu: colour sliders HUD {}, Pip-Boy {}", says(vis.colours[0]), says(vis.colours[1]));
        view.colours_shown = Some(vis.colours);
    }
    if sel != selected {
        *world.resource_mut::<Page>() = Page::Settings { selected: sel };
    }
    (sel, entries, rail)
}

/// What the page shows now.
fn drawn_now(world: &mut World) -> Drawn {
    let view_w = world.query_filtered::<&bevy::window::Window, With<bevy::window::PrimaryWindow>>().iter(world).next().filter(|w| w.height() > 0.0).map_or(VIEW_W, |w| 960.0 * w.width() / w.height()).round() as u32;
    let list = world.resource::<Settings>().list().to_vec();
    let states: Vec<f32> = world.resource::<Settings>().values(world);
    if matches!(*world.resource::<Page>(), Page::Settings { .. }) {
        if world.resource::<PauseView>().defaults.is_none() {
            let start = world.get_resource::<crate::settings::StartStates>().map(|s| s.0.clone()).filter(|s| s.len() == states.len());
            // Without the start's states the lists' states are taken now too, as the INI
            // keeps them (what Defaults and the Changed list go by).
            if start.is_none() && !world.contains_resource::<crate::settings::StartChoices>() {
                let now = crate::settings::choices_now(world);
                world.insert_resource(now);
            }
            world.resource_mut::<PauseView>().defaults = Some(start.unwrap_or_else(|| states.clone()));
        }
        changed_now(world, &list);
    }
    let (page, lines, rail) = match *world.resource::<Page>() {
        Page::Settings { selected } => {
            let (sel, lines, rail) = settings_now(world, &list, selected);
            (Page::Settings { selected: sel }, lines, rail)
        }
        page => (page, Vec::new(), Vec::new()),
    };
    let disabled: Vec<_> = (0..list.len()).map(|i| world.resource::<Settings>().disabled_status(world, i)).collect();
    let lists: Vec<(usize, Vec<String>, String)> = {
        let settings = world.resource::<Settings>();
        (0..list.len()).filter_map(|i| settings.choices(i).map(|c| (i, (c.names)(world), (c.chosen)(world)))).collect()
    };
    let looks: Vec<(usize, ListLooks)> = {
        let settings = world.resource::<Settings>();
        lists
            .iter()
            .filter(|(i, _, _)| settings.choice_looks(*i).is_some())
            .map(|(i, names, chosen)| {
                let i = *i;
                let looks = ListLooks {
                    labels: names.iter().map(|n| settings.choice_label(world, i, n)).collect(),
                    reasons: names.iter().map(|n| settings.choice_unavailable(world, i, n)).collect(),
                    chosen: (settings.choice_label(world, i, chosen), settings.choice_unavailable(world, i, chosen)),
                };
                (i, looks)
            })
            .collect()
    };
    let keep = world.get_resource::<crate::settings::PendingKeep>().map(|p| {
        let now = world.get_resource::<Time<Real>>().map_or(0.0, |t| t.elapsed_secs_f64());
        (list.get(p.row).map_or("choice", |s| s.label), p.left(now), world.resource::<PauseView>().keep_selected)
    });
    let main = world.get_resource::<crate::main_menu::MainMenu>().filter(|m| m.open).map(|m| m.column());
    let stand_ins: Vec<(RowId, Vec<String>)> = {
        let view = world.resource::<PauseView>();
        let ids = match view.scope {
            SettingsScope::Search => view.found.search.as_slice(),
            SettingsScope::Changed => view.found.changed.as_slice(),
            SettingsScope::Tab(_) => &[],
        };
        let vis = Visibility::of_world(world);
        crate::settings_view::stands_for(&list, ids, vis).into_iter().map(|(by, rows)| (by, rows.into_iter().map(|r| stand_in_name(&list, r)).collect())).collect()
    };
    // Over a game only, on the first page and the settings page (the load page's rows
    // and a mod's page have the bottom left).
    let report_shown = main.is_none() && matches!(page, Page::Main | Page::Settings { .. }) && world.get_resource::<ReportEntry>().is_some_and(|r| r.shown);
    let view = world.resource::<PauseView>();
    let thumb = world.resource::<LoadList>().thumb.as_ref().map(|(i, h)| (*i, h.is_some()));
    Drawn {
        page,
        entries: lines,
        scope: view.scope,
        focus: view.focus,
        rail,
        rail_hover: view.rail_hover.filter(|_| matches!(page, Page::Settings { .. })),
        listening: world.get_resource::<Listening>().and_then(|l| l.0).map(|l| l.0),
        main_selected: view.main_selected,
        confirm: view.confirm,
        states,
        disabled,
        status: world.resource::<PauseStatus>().line.clone(),
        under_main: main.is_some(),
        mod_window: page == Page::Mods && world.get_resource::<crate::tiles::mods::ModsPage>().is_some_and(|m| m.mcm_open),
        column: main.unwrap_or_else(|| entries(world.get_resource::<ModsEntry>().is_some_and(|m| m.0)).iter().map(|e| (e.label(), true)).collect()),
        thumb,
        controls: world.get_resource::<crate::bindings::Bindings>().map(|b| b.all().map(|(c, _)| b.text_on(c)).collect()).unwrap_or_default(),
        device: world.get_resource::<crate::bindings::Bindings>().map_or(("Keyboard and Mouse", false), |b| (b.device.name(), b.device == crate::bindings::InputDevice::Controller)),
        automatic_input: world.get_resource::<crate::bindings::Bindings>().is_some_and(|b| b.automatic_input_switching),
        rumble: world.get_resource::<crate::bindings::Bindings>().is_some_and(|b| b.rumble),
        invert_y: world.get_resource::<crate::bindings::Bindings>().is_some_and(|b| b.invert_y),
        sensitivity: world.get_resource::<crate::bindings::Bindings>().map_or(1.0, |b| b.sensitivity()),
        vertical: world.get_resource::<crate::bindings::Bindings>().map_or(1.0, |b| b.vertical_sensitivity()),
        scroll: view.scroll,
        lines: view.lines(),
        lists,
        looks,
        dropdown: view.dropdown.filter(|_| matches!(page, Page::Settings { .. })),
        keep,
        delete_ask: view.delete_ask.filter(|_| matches!(page, Page::Load { .. })),
        view_w,
        language: fnv_util::words::generation(),
        search: (matches!(page, Page::Settings { .. }) && view.scope == SettingsScope::Search).then(|| (view.search.query.clone(), view.focus == SettingsFocus::SearchText)),
        stand_ins,
        report: report_shown.then_some(view.report_hover),
    }
}

/// The page's name for row `id` where another row stands for it: a setting's label, a
/// control's with its device ("Jump (Controller)"), the sensitivity of its device.
fn stand_in_name(list: &[Setting], id: RowId) -> String {
    use crate::bindings::InputDevice;
    match id {
        RowId::Setting(i) => list.get(i).map_or_else(String::new, |s| tr(s.label).to_string()),
        RowId::Control(c) => {
            let controller = matches!(c, ControlRow::Binding(_, InputDevice::Controller) | ControlRow::Sensitivity(InputDevice::Controller) | ControlRow::Vertical(InputDevice::Controller) | ControlRow::Rumble);
            let d = Drawn { device: ("", controller), ..default() };
            let (label, _) = control_line(&d, c);
            match c {
                ControlRow::Binding(_, dev) | ControlRow::Vertical(dev) => format!("{label} ({})", tr(dev.name())),
                _ => label.to_string(),
            }
        }
    }
}

/// Row `id`'s note, after the names of the rows it stands for on the search or the Changed
/// list ("Stands for: Jump (Controller)."), so a choice there is plain [design: #1953].
fn note_of(list: &[Setting], id: RowId, d: &Drawn) -> String {
    let note = row_note(list, id, d);
    match d.stand_ins.iter().find(|(by, _)| *by == id) {
        Some((_, names)) => {
            // Three names at most, so the head and the note keep to the note's lines (#2377);
            // the page's state keeps them all.
            let head = if names.len() > STAND_IN_NAMES {
                let more = names.len() - STAND_IN_NAMES;
                fnv_util::words::trf("Stands for: {rows} and {n} more.", &[("rows", &names[..STAND_IN_NAMES].join(", ")), ("n", &more)])
            } else {
                fnv_util::words::trf("Stands for: {rows}.", &[("rows", &names.join(", "))])
            };
            if note.is_empty() { head } else { format!("{head} {note}") }
        }
        None => note,
    }
}

/// The Changed list's rows now ([`PauseView::found`]): the registered rows away from
/// Defaults' states (`crate::settings::changed_rows`) and the Controls rows away from the
/// game's (`crate::bindings::changed_controls`), in the page's order.
fn changed_now(world: &mut World, list: &[Setting]) {
    let base = world.resource::<PauseView>().defaults.clone().unwrap_or_default();
    let rows = crate::settings::changed_rows(world, &base);
    let controls = world.get_resource::<crate::bindings::Bindings>().map(crate::bindings::changed_controls).unwrap_or_default();
    if !world.contains_resource::<SettingsLayout>() {
        world.init_resource::<SettingsLayout>();
    }
    let changed = world.resource_scope(|_, mut layout: Mut<SettingsLayout>| crate::settings_view::changed(layout.refresh(list), &rows, |c| controls.contains(&c)));
    world.resource_mut::<PauseView>().found.changed = changed;
}

/// The values a registered row can show, as the page draws them (translated): a switch's ON
/// and OFF, a slider's value at each of its steps, a list's choices by their words, and the
/// reason it is dimmed now, each with its kind for [`row_parts`] (`width_of`: the entries'
/// font).
fn row_values(world: &World, settings: &Settings, i: usize, width_of: impl Fn(&str) -> f32) -> Vec<(ValueKind, String)> {
    let s = settings.list()[i];
    let mut out = Vec::new();
    if let Some(reason) = settings.disabled_status(world, i) {
        let reason = tr(reason).to_string();
        out.push((ValueKind::Centred(width_of(&reason)), reason));
    }
    if let Some(sl) = s.slider {
        let steps = (((1.0 - sl.min) / sl.step.max(1e-3)).round() as usize).min(1000);
        for k in 0..=steps {
            let words = tr_str(&sl.text(sl.fit(sl.min + k as f32 * sl.step))).to_string();
            out.push((ValueKind::Slider(width_of(&words)), words));
        }
    } else if let Some(c) = settings.choices(i) {
        for name in (c.names)(world) {
            let words = tr_str(&settings.choice_label(world, i, &name)).to_string();
            out.push((ValueKind::List(width_of(&words)), words));
        }
    } else {
        for v in ["ON", "OFF"] {
            out.push((ValueKind::Centred(width_of(tr(v))), tr(v).to_string()));
        }
    }
    out.dedup_by(|a, b| a.1 == b.1);
    out
}

/// The settings page's words checked with its faces `f`, once they are loaded (the first time
/// the pause menu draws) [design: #2377]: every registered row laid out at the page's width
/// in a 16:9 view, the rail as wide as its words; a row whose label or one of its values has
/// to be cut to fit logs `settings: text of `<key>` cut to fit: …`, then `settings: <n> rows
/// laid out, <cut> cut to fit`. Cutting is how the page keeps text from running into its
/// neighbour; the log says where a label or a value's words are too long for the page.
fn check_page_text(world: &mut World, f: &Faces) {
    let Some(list) = world.get_resource::<Settings>().map(|s| s.list().to_vec()) else { return };
    if !world.contains_resource::<SettingsLayout>() {
        world.init_resource::<SettingsLayout>();
    }
    let rail = world.resource_scope(|_, mut layout: Mut<SettingsLayout>| crate::settings_view::rail(layout.refresh(&list)));
    let lb = ListBox::of(VIEW_W, rail_width(f, &rail, false));
    let x = lb.row();
    let width = |t: &str| f.entry.font.width(t);
    let settings = world.resource::<Settings>();
    let mut cut = 0;
    for (i, s) in list.iter().enumerate() {
        let label = tr(s.label);
        let mut what = Vec::new();
        for (kind, value) in row_values(world, settings, i, width) {
            let parts = row_parts(x, kind);
            let (l, v) = fit_row(width, &parts, label, &value);
            if l != label && !what.iter().any(|w: &String| w.starts_with("label")) {
                what.push(format!("label \"{label}\" ({:.0} of {:.0} units)", width(label), parts.label_room()));
            }
            if v != value {
                what.push(format!("value \"{value}\" ({:.0} of {:.0} units)", width(&value), parts.text_room()));
            }
        }
        if !what.is_empty() {
            cut += 1;
            warn!("settings: text of `{}` cut to fit: {}", s.key, what.join(", "));
        }
    }
    info!("settings: {} rows laid out, {cut} cut to fit (a list {:.0} units wide in a 16:9 view)", list.len(), lb.width);
    // The notes: within the budget and two lines at the page's width in the small face
    // (`crate::settings::note_over_budget` checks the characters at the start too).
    let mut bad = 0;
    for s in &list {
        let note = tr(s.note);
        let lines = f.small.font.lines(note, Some(note_width(lb)));
        let widest = lines.iter().map(|l| l.width).fold(0.0, f32::max);
        let reason = crate::settings::note_over_budget(note)
            .or_else(|| (lines.len() > NOTE_ROW_LINES).then(|| format!("{} lines at the page's width", lines.len())))
            .or_else(|| (widest > note_width(lb)).then(|| format!("a word {widest:.0} units wide, wider than the page's {:.0}", note_width(lb))));
        if let Some(reason) = reason {
            bad += 1;
            warn!("settings: note of `{}` over the budget: {reason}", s.key);
        }
    }
    info!("settings: {} notes checked, {bad} over the budget ({} characters, two lines)", list.len(), crate::settings::NOTE_BUDGET);
}

/// The settings list's place, logged when it changes (a new window size, a translated rail):
/// `pause menu: settings list <width> wide from <left> in a view <w> wide, band <l>-<r>`, in
/// interface units from the view's left edge [design: #2377].
fn log_list_box(view: &mut PauseView, lb: ListBox, view_w: u32) {
    if view.list_logged == Some(lb) {
        return;
    }
    let half = view_w as f32 / 2.0;
    let (band_left, band_w) = lb.band();
    info!("pause menu: settings list {:.0} wide from {:.0} in a view {view_w} wide, band {:.0}-{:.0}", lb.width, half + lb.left, half + band_left, half + band_left + band_w);
    view.list_logged = Some(lb);
}

/// Draw the page again when what it shows changed.
fn draw(world: &mut World) {
    // A film holds the game as the pause menu does (`crate::intro`) but shows no page.
    if !world.resource::<Paused>().0 || world.get_resource::<crate::intro::Intro>().is_some_and(|i| i.is_playing()) {
        return;
    }
    let now = drawn_now(world);
    if world.resource::<PauseView>().drawn.as_ref() == Some(&now) {
        return;
    }
    if !world.contains_resource::<PauseLook>() {
        let look = PauseLook::load(world);
        world.insert_resource(look);
        // The page's words checked once with the faces they are drawn in (#2377).
        world.resource_scope(|world, look: Mut<PauseLook>| check_page_text(world, &look.faces));
    }
    let roots: Vec<Entity> = world.query_filtered::<Entity, With<PauseRoot>>().iter(world).collect();
    for e in roots {
        world.despawn(e);
    }
    let settings = world.resource::<Settings>().list().to_vec();
    let glyphs = world.resource::<crate::pad::PadGlyphs>().clone();
    if matches!(now.page, Page::Settings { .. }) {
        let lb = ListBox::of(now.view_w as f32, rail_width(&world.resource::<PauseLook>().faces, &now.rail, now.device.1));
        log_list_box(&mut world.resource_mut::<PauseView>(), lb, now.view_w);
    }
    world.resource_scope(|world, look: Mut<PauseLook>| {
        world.resource_scope(|world, list: Mut<LoadList>| {
            let mut commands = world.commands();
            spawn_page(&mut commands, &now, &look, &settings, &list, &glyphs);
        });
    });
    world.flush();
    world.resource_mut::<PauseView>().drawn = Some(now);
}

#[allow(clippy::too_many_arguments)]
fn close(mut commands: Commands, mut paused: ResMut<Paused>, mut page: ResMut<Page>, mut list: ResMut<LoadList>, mut view: ResMut<PauseView>, mut listening: ResMut<Listening>, roots: Query<Entity, With<PauseRoot>>, mut resume: ResMut<crate::graphics_resume::GraphicsResume>) {
    resume.reset();
    paused.0 = false;
    *page = Page::Main;
    listening.0 = None;
    view.scope = SettingsScope::default();
    view.focus = SettingsFocus::Rows;
    view.colours_shown = None;
    *list = LoadList::default();
    view.drawn = None;
    view.confirm = None;
    view.dragging = None;
    view.bar_dragging = false;
    view.scroll = 0;
    view.followed = None;
    view.rail_hover = None;
    view.report_hover = false;
    view.dropdown = None;
    view.search = SearchState::default();
    view.found = Found::default();
    view.changed_logged = None;
    for e in &roots {
        commands.entity(e).despawn();
    }
}

/// Both Continue/Esc and the timed aid request the same release [design: this file].
fn request_resume(world: &mut World) {
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    if world.resource_mut::<crate::graphics_resume::GraphicsResume>().request(now) {
        release_resume(world);
    }
}

fn reset_resume(world: &mut World) {
    world.resource_mut::<crate::graphics_resume::GraphicsResume>().reset();
}

fn cancel_resume(world: &mut World) {
    if world.resource::<crate::graphics_resume::GraphicsResume>().pending() {
        info!("pause menu: resume cancelled, another choice was made");
        world.resource_mut::<crate::graphics_resume::GraphicsResume>().cancel();
    }
}

fn release_resume(world: &mut World) {
    info!("pause menu: resume");
    world.resource_mut::<NextState<PlayState>>().set(PlayState::Playing);
}

fn finish_resume(world: &mut World) {
    // A cover/load may start earlier in this frame, before OnExit closes this menu.
    // Do not replace that pending transition with Playing [design: loading.rs].
    if !world.resource::<Paused>().0
        || matches!(world.resource::<NextState<PlayState>>(), NextState::Pending(state) if *state != PlayState::Menu)
        || world.get_resource::<crate::loading::Loading>().is_some_and(|loading| loading.active())
    {
        reset_resume(world);
        return;
    }
    // The wait belongs to the bare column Continue was chosen on. A page or the quit
    // question opened since (by any path: a choice, another menu's page request, a test
    // aid) ends it rather than being closed by the release. The keep question is left
    // out: it outlives the menu (`crate::settings::revert_unkept`).
    let column = *world.resource::<Page>() == Page::Main && world.resource::<PauseView>().confirm.is_none();
    if !column {
        cancel_resume(world);
        return;
    }
    let now = world.resource::<Time<Real>>().elapsed_secs_f64();
    if let Some(done) = world.resource_mut::<crate::graphics_resume::GraphicsResume>().poll(now) {
        info!("pause menu: resume after graphics change: pipelines waited {:.2} s, pending {}, capped {}", done.waited, done.pending, done.capped);
        release_resume(world);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_form_shades_the_menu_under_it_and_its_panel_hides_it() {
        use super::{form_panel, form_shade, shown_under, FORM_SHADE};
        // The pictures' numbers: 0.7 showed the menu's amber at 149 of 255, 0.95 at 14 % more.
        assert!((shown_under(0.7) * 255.0 - 149.0).abs() < 1.0, "{}", shown_under(0.7) * 255.0);
        assert!(shown_under(FORM_SHADE) <= 0.35, "the menu under a form reads at a third at most");
        assert_eq!(form_shade().0.alpha(), FORM_SHADE);
        assert_eq!(form_panel().0.alpha(), 1.0, "nothing shows through the panel");
    }

    use super::*;

    /// Issue #1893: Back carries its hint, the keyboard's Esc or the controller's B.
    #[test]
    fn back_names_its_key_and_button() {
        use crate::pad::Labels;
        assert_eq!(back_label_as(Labels::Keys), format!("{}  Esc)", tr("Back")));
        assert_eq!(back_label_as(Labels::Xbox), format!("{}  B)", tr("Back")));
        assert_eq!(back_label_as(Labels::PlayStation), format!("{}  Circle)", tr("Back")));
    }

    fn resume_app() -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>().init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Time>().init_resource::<Time<Real>>().init_resource::<PlayClock>()
            .init_resource::<PauseAid>().insert_resource(Paused(true)).init_resource::<Page>()
            .init_resource::<PauseView>().init_resource::<Settings>().init_resource::<NextState<PlayState>>()
            .insert_resource(State::new(PlayState::Menu)).init_resource::<crate::graphics_resume::GraphicsResume>()
            .init_resource::<PageRequest>().init_resource::<LoadList>().init_resource::<PauseStatus>()
            .init_resource::<Listening>().init_resource::<SettingsLayout>()
            .init_resource::<Assets<Image>>().insert_resource(crate::save::SaveConfig::with_dir(None))
            .add_message::<crate::save::SaveRequest>().add_message::<crate::save::LoadRequest>()
            .add_message::<MainMenuRequest>().add_message::<AppExit>().add_message::<crate::ui_sounds::UiSound>()
            .add_systems(Update, (choose, finish_resume).chain());
        app
    }

    #[test]
    fn both_resume_paths_hold_the_pause_until_ready_and_unchanged_releases_immediately() {
        // Timed aid, Esc and Continue all run through choose, including deferred requests.
        for path in 0..3 {
            for changed in [false, true] {
                let mut app = resume_app();
                if changed {
                    app.world_mut().resource_mut::<crate::graphics_resume::GraphicsResume>().changed();
                }
                match path {
                    0 => app.world_mut().resource_mut::<PauseAid>().resume.push(0.0),
                    1 => app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape),
                    _ => app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Enter),
                }
                app.update();
                assert_eq!(app.world().resource::<crate::graphics_resume::GraphicsResume>().pending(), changed);
                if changed {
                    assert!(matches!(app.world().resource::<NextState<PlayState>>(), NextState::Unchanged));
                    assert!(app.world().resource::<Paused>().0);
                    assert_eq!(*app.world().resource::<State<PlayState>>().get(), PlayState::Menu);
                    assert!(picture_held(MenuPause::Living, true, PlayState::Menu));
                    // Repeated keys/requests do not replace the pending wait.
                    app.update();
                    app.world_mut().resource_mut::<crate::graphics_resume::GraphicsResume>().sample(1, 0, 0);
                    app.update();
                    app.world_mut().resource_mut::<Time<Real>>().advance_by(std::time::Duration::from_secs_f64(0.3));
                    app.world_mut().resource_mut::<crate::graphics_resume::GraphicsResume>().sample(4, 0, 0);
                    app.update();
                }
                assert!(matches!(app.world().resource::<NextState<PlayState>>(), NextState::Pending(PlayState::Playing)));
                assert!(!app.world().resource::<crate::graphics_resume::GraphicsResume>().pending());
            }
        }
    }

    #[test]
    fn a_quit_or_menu_closure_discards_the_pending_resume() {
        let mut app = resume_app();
        app.world_mut().resource_mut::<crate::graphics_resume::GraphicsResume>().changed();
        request_resume(app.world_mut());
        app.world_mut().resource_mut::<PauseView>().confirm = Some(1);
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Enter);
        app.update();
        assert!(!app.world().resource::<crate::graphics_resume::GraphicsResume>().pending());
        assert!(matches!(app.world().resource::<NextState<PlayState>>(), NextState::Unchanged));
        app.world_mut().resource_mut::<crate::graphics_resume::GraphicsResume>().changed();
        request_resume(app.world_mut());
        use bevy::ecs::system::RunSystemOnce;
        app.world_mut().run_system_once(close).unwrap();
        assert!(!app.world().resource::<Paused>().0);
        assert!(app.world_mut().resource_mut::<crate::graphics_resume::GraphicsResume>().request(1.0));
    }

    #[test]
    fn a_load_discards_the_wait_and_the_cap_releases_through_the_same_system() {
        let mut app = resume_app();
        app.world_mut().resource_mut::<crate::graphics_resume::GraphicsResume>().changed();
        request_resume(app.world_mut());
        *app.world_mut().resource_mut::<Page>() = Page::Load { selected: 0 };
        {
            let mut list = app.world_mut().resource_mut::<LoadList>();
            list.saves.push((PathBuf::from("synthetic-resume-save.nvf"), SaveHeader::new(SaveKind::Manual)));
            list.thumb = Some((0, None));
        }
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Enter);
        app.update();
        assert!(!app.world().resource::<crate::graphics_resume::GraphicsResume>().pending());
        assert!(matches!(app.world().resource::<NextState<PlayState>>(), NextState::Unchanged));
        assert_eq!(app.world().resource::<Messages<crate::save::LoadRequest>>().len(), 1);

        let mut app = resume_app();
        app.world_mut().resource_mut::<crate::graphics_resume::GraphicsResume>().changed();
        request_resume(app.world_mut());
        app.world_mut().resource_mut::<crate::graphics_resume::GraphicsResume>().sample(1, 2, 2);
        app.world_mut().resource_mut::<Time<Real>>().advance_by(std::time::Duration::from_secs_f64(crate::loading::PIPELINE_CAP_SECS));
        app.update();
        assert!(!app.world().resource::<crate::graphics_resume::GraphicsResume>().pending());
        assert!(matches!(app.world().resource::<NextState<PlayState>>(), NextState::Pending(PlayState::Playing)));

        let mut app = resume_app();
        app.world_mut().resource_mut::<crate::graphics_resume::GraphicsResume>().changed();
        request_resume(app.world_mut());
        app.world_mut().resource_mut::<Time<Real>>().advance_by(std::time::Duration::from_secs_f64(crate::loading::PIPELINE_CAP_SECS));
        app.world_mut().resource_mut::<NextState<PlayState>>().set(PlayState::Loading);
        app.update();
        assert!(!app.world().resource::<crate::graphics_resume::GraphicsResume>().pending());
        assert!(matches!(app.world().resource::<NextState<PlayState>>(), NextState::Pending(PlayState::Loading)), "a load beats a ready or capped resume in the same frame");
    }

    /// One fresh key press for the next update (the test app has no input system).
    fn press_only(app: &mut App, key: KeyCode) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        keys.press(key);
    }

    #[test]
    fn another_choice_during_the_wait_cancels_it_and_a_later_continue_waits_again() {
        use crate::graphics_resume::GraphicsResume;
        let cap = std::time::Duration::from_secs_f64(crate::loading::PIPELINE_CAP_SECS);
        // Settings, the quit question, Load, Save: each chosen on the column after Continue.
        for (key, back) in [(KeyCode::KeyS, true), (KeyCode::KeyQ, true), (KeyCode::KeyL, true), (KeyCode::KeyN, false)] {
            let mut app = resume_app();
            app.world_mut().resource_mut::<GraphicsResume>().changed();
            app.world_mut().resource_mut::<GraphicsResume>().sample(1, 2, 2);
            press_only(&mut app, KeyCode::Enter);
            app.update();
            assert!(app.world().resource::<GraphicsResume>().pending(), "{key:?}: Continue waits");
            press_only(&mut app, key);
            app.update();
            let opened = (*app.world().resource::<Page>(), app.world().resource::<PauseView>().confirm);
            assert!(!app.world().resource::<GraphicsResume>().pending(), "{key:?}: the choice cancels the wait");
            // The cap (and busy pipelines) pass with the page or question open.
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
            app.world_mut().resource_mut::<Time<Real>>().advance_by(cap);
            app.world_mut().resource_mut::<GraphicsResume>().sample(2, 2, 2);
            app.update();
            assert_eq!((*app.world().resource::<Page>(), app.world().resource::<PauseView>().confirm), opened, "{key:?}: still open");
            assert!(app.world().resource::<Paused>().0);
            assert!(matches!(app.world().resource::<NextState<PlayState>>(), NextState::Unchanged), "{key:?}: no release");
            if back {
                press_only(&mut app, KeyCode::Escape);
                app.update();
                assert_eq!(*app.world().resource::<Page>(), Page::Main);
                assert_eq!(app.world().resource::<PauseView>().confirm, None);
            }
            // Continue again: the change is still latched and the pipelines still busy, so it
            // waits, with the cap counted from this request.
            app.world_mut().resource_mut::<PauseView>().main_selected = 0;
            press_only(&mut app, KeyCode::Enter);
            app.update();
            assert!(app.world().resource::<GraphicsResume>().pending(), "{key:?}: the second Continue waits");
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
            app.world_mut().resource_mut::<Time<Real>>().advance_by(cap / 2);
            app.world_mut().resource_mut::<GraphicsResume>().sample(3, 2, 2);
            app.update();
            assert!(matches!(app.world().resource::<NextState<PlayState>>(), NextState::Unchanged));
            app.world_mut().resource_mut::<Time<Real>>().advance_by(cap);
            app.update();
            assert!(matches!(app.world().resource::<NextState<PlayState>>(), NextState::Pending(PlayState::Playing)), "{key:?}: released by the new cap");
        }
    }

    #[test]
    fn a_page_opened_by_another_path_during_the_wait_cancels_it_and_the_aid_resumes_from_a_page() {
        use crate::graphics_resume::GraphicsResume;
        let cap = std::time::Duration::from_secs_f64(crate::loading::PIPELINE_CAP_SECS);
        let mut app = resume_app();
        app.world_mut().resource_mut::<GraphicsResume>().changed();
        request_resume(app.world_mut());
        app.world_mut().resource_mut::<PageRequest>().0 = Some(Page::Settings { selected: None });
        app.world_mut().resource_mut::<Time<Real>>().advance_by(cap);
        app.update();
        assert!(!app.world().resource::<GraphicsResume>().pending());
        assert!(matches!(*app.world().resource::<Page>(), Page::Settings { .. }), "the settings page is open");
        assert!(matches!(app.world().resource::<NextState<PlayState>>(), NextState::Unchanged));
        // The timed aid's Esc from the settings page goes back to the column and waits there.
        app.world_mut().resource_mut::<PauseAid>().resume.push(0.0);
        app.update();
        assert_eq!(*app.world().resource::<Page>(), Page::Main);
        assert!(app.world().resource::<GraphicsResume>().pending());
        app.world_mut().resource_mut::<Time<Real>>().advance_by(cap);
        app.update();
        assert!(matches!(app.world().resource::<NextState<PlayState>>(), NextState::Pending(PlayState::Playing)));
    }

    /// The views the settings page is laid out for in the tests: 16:9 (1280x720 and 1920x1080
    /// are both 1707 units wide), 16:10, 4:3 and 3440x1440 (21:9, 2293 units).
    const VIEW_WIDTHS: [f32; 4] = [VIEW_W, 1536.0, 1280.0, 2293.0];

    #[test]
    fn the_settings_page_fits_the_view() {
        for view_w in VIEW_WIDTHS {
            for tabs_w in [SUB_WIDTH, 220.0] {
                let lb = ListBox::of(view_w, tabs_w);
                let (band_left, band_w) = lb.band();
                let band_left = view_w / 2.0 + band_left;
                let band_right = band_left + band_w;
                let tabs_left = view_w - SUB_RIGHT - tabs_w;
                assert!(band_right + TABS_GAP <= tabs_left + 0.01, "{view_w}/{tabs_w}: the band ends at {band_right}, the categories start at {tabs_left}");
                assert!(band_left >= LEFT_MARGIN - 0.01, "{view_w}/{tabs_w}: the band starts at {band_left}");
                assert!(lb.width <= OPTIONS_MAX, "{view_w}/{tabs_w}: {}", lb.width);
                // Room for a label left of the value part; the list's box (and its dropdown,
                // as wide) inside the row.
                let x = lb.row();
                assert!(x.value_left() - 10.0 >= 150.0, "{view_w}/{tabs_w}: a label's room {}", x.value_left() - 10.0);
                assert!(x.list_left() >= 0.0 && x.list_left() + LIST_WIDTH <= lb.width && x.meter_left() > x.arrows()[0] && x.percent_at() < lb.width);
            }
        }
        // 16:9 with a 220-unit rail: the list is about 1036 units wide (820 before #2377),
        // its band from LEFT_MARGIN.
        let lb = ListBox::of(VIEW_W, 220.0);
        assert!((lb.width - 1035.7).abs() < 0.5, "{}", lb.width);
        assert!((VIEW_W / 2.0 + lb.band().0 - LEFT_MARGIN).abs() < 0.01);
        // 3440x1440: capped at OPTIONS_MAX and centred in the room left of the rail.
        let lb = ListBox::of(2293.0, 220.0);
        assert_eq!(lb.width, OPTIONS_MAX);
        let room = 2293.0 - (SUB_RIGHT + 220.0 + TABS_GAP) - (BAR_GAP + BAR_W) - 2.0 * BAND_MARGIN - LEFT_MARGIN;
        assert!((2293.0 / 2.0 + lb.band().0 - (LEFT_MARGIN + (room - OPTIONS_MAX) / 2.0)).abs() < 0.01);
        // A row of the width before #2377 has every place where it was; a wider one keeps
        // each as far from its right end.
        let x = RowX(ROW_BASE);
        assert_eq!((x.value_at(), x.value_left(), x.meter_left(), x.arrows(), x.percent_at(), x.list_left()), (VALUE_AT, LIST_LEFT, METER_LEFT, METER_ARROWS, PERCENT_AT, LIST_LEFT));
        let x = RowX(1036.0);
        assert_eq!((x.0 - x.value_at(), x.0 - x.list_left(), x.0 - x.percent_at()), (ROW_BASE - VALUE_AT, ROW_BASE - LIST_LEFT, ROW_BASE - PERCENT_AT));
        // The window's lines (12 today) and a note of three lines fit above "Report a bug"
        // and the version's line, and one more line would not (#2377).
        let lines = window_lines();
        assert_eq!(lines, 12);
        let note_top = OPTIONS_TOP + lines as f32 * OPTION_ROW + NOTE_GAP;
        assert_eq!(note_fit(2, note_top), 2);
        assert_eq!(note_fit(30, note_top), NOTE_LINES);
        assert_eq!(note_fit(30, OPTIONS_TOP + NOTE_GAP), NOTE_LINES, "a short list's note is no longer either");
        assert!(note_top + NOTE_LINES as f32 * NOTE_STEP <= REPORT_TOP && REPORT_TOP < NOTE_BOTTOM && NOTE_BOTTOM < 960.0);
        assert!(note_top + OPTION_ROW + NOTE_LINES as f32 * NOTE_STEP > REPORT_TOP, "a 13th line would leave no room for the note");
    }

    /// The systems that read or write the world aids' clock (directly or through
    /// [`AidTime`]) build into a schedule: Bevy checks each system's parameters for a
    /// resource taken twice (B0002) when it is initialized, which unit tests of the clock
    /// alone never do. `start_world_aids` once took the clock in `AidTime` and as
    /// `ResMut` and every `--play` run panicked (2026-10-18). The parameters are the same
    /// whatever NVF_PLAY_AIDS_FROM_SCRIPTS says; it is set as the smoke runner sets it.
    #[test]
    fn the_world_aids_clock_systems_build_into_a_schedule() {
        // SAFETY: this test's own variable; no other test reads it.
        unsafe { std::env::set_var("NVF_PLAY_AIDS_FROM_SCRIPTS", "1") };
        let mut world = World::new();
        world.insert_resource(WorldAidClock::from_env());
        let mut schedule = Schedule::default();
        schedule.add_systems((crate::script::run_scripts, crate::script::start_world_aids, tick_play_clock).chain());
        schedule.initialize(&mut world).expect("the schedule builds");
        assert!(world.resource::<WorldAidClock>().from_scripts);
    }

    #[test]
    fn the_world_aids_clock_waits_for_the_scripts_and_is_held_back_only_by_a_late_start() {
        // Without the switch: the aids' clock, from the start.
        let mut off = WorldAidClock::new(false);
        assert_eq!(off.at(3.0), Some(3.0));
        assert_eq!(off.start(20.0), None);
        assert_eq!(off.at(25.0), Some(25.0));
        assert_eq!(off.held_secs(), 0.0);
        // With it: nothing before the scripts run.
        let mut on = WorldAidClock::new(true);
        assert_eq!(on.at(3.0), None);
        // An unbusy start (before SCRIPTS_START_SECONDS): not held back, the times mean
        // what they did; steps due already run in the first frame.
        assert_eq!(on.start(5.5), Some(0.0));
        assert_eq!(on.at(5.5), Some(5.5));
        assert_eq!(on.at(14.0), Some(14.0));
        // Only the first start counts.
        assert_eq!(on.start(30.0), None);
        assert_eq!(on.at(14.0), Some(14.0));
        // A busy start at 20 s: the clock reads SCRIPTS_START_SECONDS then and steps keep
        // their distances (a step 6 s after that runs 6 s after the start).
        let mut late = WorldAidClock::new(true);
        assert_eq!(late.start(20.0), Some(20.0 - SCRIPTS_START_SECONDS));
        assert_eq!(late.at(20.0), Some(SCRIPTS_START_SECONDS));
        assert_eq!(late.at(26.0), Some(SCRIPTS_START_SECONDS + 6.0));
        assert_eq!(late.held_secs(), 20.0 - SCRIPTS_START_SECONDS);
    }

    #[test]
    fn the_pause_menu_always_holds_the_picture_and_other_menus_by_the_switch() {
        use PlayState::*;
        for mode in [MenuPause::Living, MenuPause::Original] {
            // The pause menu (a `Menu` state with `Paused`), whatever the switch says.
            assert!(picture_held(mode, true, Menu));
            // In the world nothing is held.
            assert!(!picture_held(mode, false, Playing));
        }
        // The inventory, a container, a message box (`Menu`) and the dialogue screen.
        assert!(!picture_held(MenuPause::Living, false, Menu));
        assert!(!picture_held(MenuPause::Living, false, Dialogue));
        assert!(picture_held(MenuPause::Original, false, Menu));
        assert!(picture_held(MenuPause::Original, false, Dialogue));
    }

    #[test]
    fn a_loading_screen_holds_the_picture_in_both_modes() {
        // A load from the main menu or the pause menu clears `Paused` as the menu closes
        // under the screen; the world behind it must still stand still.
        for mode in [MenuPause::Living, MenuPause::Original] {
            assert!(picture_held(mode, false, PlayState::Loading));
        }
    }

    #[test]
    fn the_pause_aid_parses_and_fires_once() {
        let mut a = parse_pause_aid("20:24, 30:31.5").unwrap();
        assert_eq!(a, PauseAid { open: vec![20.0, 30.0], resume: vec![24.0, 31.5], ..default() });
        assert!(!fire(&mut a.open, 19.9));
        assert!(fire(&mut a.open, 20.0));
        assert!(!fire(&mut a.open, 20.1));
        assert!(fire(&mut a.open, 40.0) && a.open.is_empty());
        assert!(parse_pause_aid("20").is_err());
        assert!(parse_pause_aid("24:20").is_err());
        assert!(parse_pause_aid("x:20").is_err());
        assert_eq!(parse_pause_aid("").unwrap(), PauseAid::default());
    }

    #[test]
    fn the_settings_aid_parses() {
        assert_eq!(parse_settings_aid("21:page, 22.5:menu-pause").unwrap(), vec![(21.0, "page".to_string()), (22.5, "menu-pause".to_string())]);
        assert!(parse_settings_aid("21").is_err());
        assert!(parse_settings_aid("x:voice").is_err());
        assert!(parse_settings_aid("").unwrap().is_empty());
    }

    /// An App with the settings page up over `rows` (key, group), drawn once a frame.
    fn settings_app(rows: &[(&'static str, &'static str)], page: Page) -> App {
        let mut app = App::new();
        app.init_resource::<Lamp>().init_resource::<Paused>().insert_resource(page);
        for (key, group) in rows {
            add_setting(&mut app, Setting { key, group, label: key, note: "", switches: "", get: |w| w.resource::<Lamp>().0, set: |w, on| w.resource_mut::<Lamp>().0 = on, slider: None });
        }
        app.init_resource::<PauseView>().init_resource::<LoadList>().init_resource::<PauseStatus>().init_resource::<crate::pad::PadGlyphs>().insert_resource(PauseLook::plain());
        app.world_mut().resource_mut::<Paused>().0 = true;
        app.add_systems(Update, draw);
        app
    }

    #[test]
    fn a_tab_shows_dim_headings_that_take_a_line_and_are_never_chosen() {
        let rows = [("difficulty", "Gameplay"), ("hardcore", "Gameplay"), ("autosave-travel", "Saving"), ("god-mode", "Debug")];
        let mut app = settings_app(&rows, Page::Settings { selected: None });
        app.update();
        let world = app.world_mut();
        // The first tab, on its first row.
        assert_eq!(*world.resource::<Page>(), Page::Settings { selected: Some(RowId::Setting(0)) });
        let mut headings: Vec<String> = world.query_filtered::<&Text, With<HeadingLine>>().iter(world).map(|t| t.0.clone()).collect();
        headings.sort();
        assert_eq!(headings, ["Game", "Saving"]);
        let texts: Vec<(String, Color)> = world.query::<(&Text, &TextColor)>().iter(world).map(|(t, c)| (t.0.clone(), c.0)).collect();
        assert!(texts.contains(&("Game".to_string(), color(Shade::Dim))), "a heading is dim: {texts:?}");
        // Rows take the pointer, headings do not; Debug's row is on its own tab.
        let buttons: Vec<PauseButton> = world.query::<&PauseButton>().iter(world).copied().collect();
        let rows_drawn: Vec<RowId> = buttons.iter().filter_map(|b| if let PauseButton::Row(r) = b { Some(*r) } else { None }).collect();
        assert_eq!(rows_drawn.len(), 3, "{rows_drawn:?}");
        assert!(!rows_drawn.contains(&RowId::Setting(3)));
        // The rail: Search, Changed, the seven tabs; the heading links.
        let rail: Vec<RailItem> = buttons.iter().filter_map(|b| if let PauseButton::Rail(r) = b { Some(*r) } else { None }).collect();
        assert_eq!(rail.len(), 2 + crate::settings_layout::TAB_ORDER.len());
        assert!(buttons.contains(&PauseButton::HeadingStep(-1)) && buttons.contains(&PauseButton::HeadingStep(1)));
        // Three rows and two headings: five lines, the window's count.
        assert_eq!(world.resource::<PauseView>().drawn.as_ref().unwrap().entries.len(), 5);
    }

    #[test]
    fn report_a_bug_is_drawn_over_the_version_only_when_the_form_is_there() {
        let drawn = |app: &mut App| {
            app.update();
            let world = app.world_mut();
            let reports: Vec<PauseButton> = world.query::<&PauseButton>().iter(world).copied().filter(|b| *b == PauseButton::Report).collect();
            let version = world.query::<&Text>().iter(world).any(|t| t.0.starts_with("NVForever"));
            (reports, version)
        };
        // Without the form (a viewer or menu-only run): no entry, and no version on the first page.
        let mut app = settings_app(&[("difficulty", "Gameplay")], Page::Main);
        assert_eq!(drawn(&mut app), (vec![], false));
        // With it: once on the first page, over the version; the settings page has it too.
        for page in [Page::Main, Page::Settings { selected: None }] {
            let mut app = settings_app(&[("difficulty", "Gameplay")], page);
            app.insert_resource(ReportEntry { shown: true, asked: false });
            let (reports, version) = drawn(&mut app);
            assert_eq!(reports.len(), 1, "{page:?}");
            assert!(version, "{page:?}: the version under it");
        }
        // Not on the load page, whose rows reach the bottom.
        let mut app = settings_app(&[("difficulty", "Gameplay")], Page::Load { selected: 0 });
        app.insert_resource(ReportEntry { shown: true, asked: false });
        assert!(drawn(&mut app).0.is_empty());
    }

    #[test]
    fn report_a_bug_points_like_the_other_buttons() {
        assert_eq!(parse_point("report"), Some(PointAt::Button(PauseButton::Report)));
    }

    #[test]
    fn an_empty_tab_says_so_and_chooses_nothing() {
        let mut app = settings_app(&[("difficulty", "Gameplay")], Page::Settings { selected: None });
        app.world_mut().resource_mut::<PauseView>().scope = SettingsScope::Tab("Tweaks");
        app.update();
        let world = app.world_mut();
        assert_eq!(*world.resource::<Page>(), Page::Settings { selected: None });
        assert_eq!(world.resource::<PauseView>().scope, SettingsScope::Tab("Tweaks"));
        let texts: Vec<String> = world.query::<&Text>().iter(world).map(|t| t.0.clone()).collect();
        // The stand-in font keeps the spaces between words (#2298).
        assert!(texts.iter().any(|t| t == "No settings here."), "{texts:?}");
        assert!(!world.query::<&PauseButton>().iter(world).any(|b| matches!(b, PauseButton::Row(_))));
        assert_eq!(Page::Settings { selected: None }.settings_row(), Some(NO_ROW_SOUND));
    }

    #[test]
    fn a_row_chosen_on_another_tab_shows_its_tab_and_a_hidden_one_its_colour() {
        // A test aid's row of another tab: its tab is shown (Debug's, here).
        let rows = [("difficulty", "Gameplay"), ("god-mode", "Debug"), ("hud-colour", "Interface"), ("hud-red", "Interface")];
        let mut app = settings_app(&rows, Page::Settings { selected: Some(RowId::Setting(1)) });
        app.update();
        assert_eq!(app.world().resource::<PauseView>().scope, SettingsScope::Tab("Debug"));
        // The HUD colour's red while the colour is a preset: hidden, its colour row chosen
        // (the colours are the game's globals; amber unless a script changed them).
        if crate::ui_colour::custom_shown(None, 0) {
            return;
        }
        *app.world_mut().resource_mut::<Page>() = Page::Settings { selected: Some(RowId::Setting(3)) };
        app.update();
        assert_eq!(*app.world().resource::<Page>(), Page::Settings { selected: Some(RowId::Setting(2)) });
        assert_eq!(app.world().resource::<PauseView>().scope, SettingsScope::Tab("Display"));
        // Custom chosen: the sliders show, the choice stays on the colour, red is listed.
        app.world_mut().insert_resource(crate::ui_colour::CustomColours([true, false]));
        app.update();
        let entries = app.world().resource::<PauseView>().drawn.as_ref().unwrap().entries.clone();
        assert_eq!(*app.world().resource::<Page>(), Page::Settings { selected: Some(RowId::Setting(2)) });
        assert!(entries.contains(&ViewEntry::Row(RowId::Setting(3))));
    }

    /// The settings page's systems build into one schedule with the pause menu's others:
    /// Bevy checks their parameters for a resource taken twice when the schedule is
    /// initialized (`choose` takes the page's layout and the rebinding state too).
    #[test]
    fn the_settings_pages_systems_build_into_a_schedule() {
        let mut world = World::new();
        let mut schedule = Schedule::default();
        schedule.add_systems((choose, show_status, draw).chain());
        schedule.initialize(&mut world).expect("the schedule builds");
    }

    /// The search's keys and the aid's input build into schedules too.
    #[test]
    fn the_search_input_systems_build_into_a_schedule() {
        let mut world = World::new();
        let mut schedule = Schedule::default();
        schedule.add_systems((type_aid, collect_search_keys, pointer_aid).chain());
        schedule.initialize(&mut world).expect("the schedule builds");
    }

    #[test]
    fn the_search_takes_typed_text_and_backspace_but_no_control_or_chord() {
        let key = |code, logical: Key, text: Option<&str>, state| KeyboardInput { state, ..aid_key(code, logical, text) };
        let events = [
            key(KeyCode::KeyN, Key::Character("n".into()), Some("n"), ButtonState::Pressed),
            key(KeyCode::KeyN, Key::Character("n".into()), None, ButtonState::Released),
            KeyboardInput { repeat: true, ..key(KeyCode::KeyN, Key::Character("n".into()), Some("n"), ButtonState::Pressed) },
            key(KeyCode::Space, Key::Space, Some(" "), ButtonState::Pressed),
            key(KeyCode::Enter, Key::Enter, Some("\r"), ButtonState::Pressed),
            key(KeyCode::Backspace, Key::Backspace, Some("\u{8}"), ButtonState::Pressed),
            key(KeyCode::ArrowDown, Key::ArrowDown, None, ButtonState::Pressed),
            key(KeyCode::Escape, Key::Escape, Some("\u{1b}"), ButtonState::Pressed),
        ];
        let t = |s: &str| SearchKey::Text(s.to_string());
        assert_eq!(search_keys(&events, false), [t("n"), t("n"), t(" "), SearchKey::Enter, SearchKey::Backspace, SearchKey::Escape]);
        // With Ctrl (Ctrl+F opens the search) or Alt held, no text.
        assert_eq!(search_keys(&events, true), [SearchKey::Enter, SearchKey::Backspace, SearchKey::Escape]);
        // The aid types each character with its key, for the keys' state too.
        let typed = typed_keys("nq W-");
        assert_eq!(typed.iter().map(|k| k.key_code).collect::<Vec<_>>()[..4], [KeyCode::KeyN, KeyCode::KeyQ, KeyCode::Space, KeyCode::KeyW]);
        assert_eq!(search_keys(&typed, false), [t("n"), t("q"), t(" "), t("W"), t("-")]);
    }

    #[test]
    fn the_pointer_aid_names_what_is_drawn() {
        assert_eq!(parse_point("heading-down"), Some(PointAt::Button(PauseButton::HeadingStep(1))));
        assert_eq!(parse_point(" changed "), Some(PointAt::Button(PauseButton::Rail(RailItem::Changed))));
        assert_eq!(parse_point("tab:Debug"), Some(PointAt::Tab("Debug".into())));
        assert_eq!(parse_point("row:menu-pause"), Some(PointAt::Row("menu-pause".into())));
        assert_eq!(parse_point("choice:2"), Some(PointAt::Choice(2)));
        assert_eq!(parse_point("choice:x"), None);
        assert_eq!(parse_point("nowhere"), None);
        let list = [Setting { key: "lamp", group: "Test", label: "Lamp", note: "", switches: "", get: |_| false, set: |_, _| {}, slider: None }];
        assert!(points_at(&PointAt::Tab("debug".into()), PauseButton::Rail(RailItem::Tab("Debug")), &list));
        assert!(points_at(&PointAt::Row("lamp".into()), PauseButton::Row(RowId::Setting(0)), &list));
        let jump = RowId::Control(ControlRow::Binding(crate::bindings::Control::Jump, crate::bindings::InputDevice::KeyboardMouse));
        assert!(points_at(&PointAt::Row("bind:Jump".into()), PauseButton::Row(jump), &list));
        assert!(!points_at(&PointAt::Row("lamp".into()), PauseButton::Meter(RowId::Setting(0)), &list), "the row, not its meter");
    }

    #[test]
    fn a_controls_row_is_searched_by_its_label_note_and_ini_name() {
        use crate::bindings::{Control, InputDevice};
        let jump = control_words(ControlRow::Binding(Control::Jump, InputDevice::KeyboardMouse));
        assert!(jump.contains("jump") && jump.contains("key or mouse button"), "{jump}");
        let pad = control_words(ControlRow::Binding(Control::Jump, InputDevice::Controller));
        assert!(pad.contains("controller button"), "{pad}");
        assert!(control_words(ControlRow::Sensitivity(InputDevice::Controller)).contains("stick sensitivity"));
        assert!(control_words(ControlRow::Invert).contains("invert y"));
    }

    #[test]
    fn the_search_lists_its_results_under_where_they_live_and_says_when_none() {
        let rows = [("difficulty", "Gameplay"), ("menu-pause", "Gameplay"), ("god-mode", "Debug")];
        let mut app = settings_app(&rows, Page::Settings { selected: None });
        {
            let mut view = app.world_mut().resource_mut::<PauseView>();
            view.scope = SettingsScope::Search;
            view.focus = SettingsFocus::SearchText;
            view.search = SearchState { query: "pause".into(), from: Some((SettingsScope::default(), None, 0)), ..default() };
        }
        app.update();
        let world = app.world_mut();
        let d = world.resource::<PauseView>().drawn.clone().unwrap();
        let line = |e: &ViewEntry| match e {
            ViewEntry::Heading(p) => format!("[{}]", p.path()),
            ViewEntry::Row(RowId::Setting(i)) => rows[*i].0.to_string(),
            ViewEntry::Row(r) => format!("{r:?}"),
        };
        // Under the heading of where it lives, its tab too.
        let names: Vec<String> = d.entries.iter().map(line).collect();
        assert_eq!(names, ["[Gameplay > Dialogue]", "menu-pause"]);
        assert_eq!(d.search, Some(("pause".to_string(), true)));
        assert_eq!(*world.resource::<Page>(), Page::Settings { selected: Some(RowId::Setting(1)) });
        // The headings name the tab too; the search's line shows the text and the mark.
        let headings: Vec<String> = world.query_filtered::<&Text, With<HeadingLine>>().iter(world).map(|t| t.0.clone()).collect();
        assert!(headings.contains(&"Gameplay > Dialogue".to_string()), "{headings:?}");
        let field: Vec<String> = world.query_filtered::<&Text, With<SearchText>>().iter(world).map(|t| t.0.clone()).collect();
        // Its start is left out when it does not fit (the test's stand-in font is wider
        // than the game's): what is shown is the query's end and the mark.
        assert_eq!(field.len(), 1, "{field:?}");
        let shown = field[0].strip_prefix("Search: ").expect("the line starts with its name");
        assert!(shown.len() > 1 && "pause_".ends_with(shown), "{field:?}");
        assert!(world.query::<&PauseButton>().iter(world).any(|b| *b == PauseButton::SearchField));
        // Nothing found: the page says so and chooses nothing.
        world.resource_mut::<PauseView>().search.query = "zzz".into();
        app.update();
        let world = app.world_mut();
        assert_eq!(*world.resource::<Page>(), Page::Settings { selected: None });
        let texts: Vec<String> = world.query::<&Text>().iter(world).map(|t| t.0.clone()).collect();
        assert!(texts.iter().any(|t| t == "No results."), "{texts:?}");
    }

    #[test]
    fn the_changed_list_shows_the_rows_away_from_defaults_and_empties_as_they_go_back() {
        let rows = [("difficulty", "Gameplay"), ("god-mode", "Debug")];
        let mut app = settings_app(&rows, Page::Settings { selected: None });
        app.world_mut().resource_mut::<PauseView>().scope = SettingsScope::Changed;
        app.update();
        let world = app.world_mut();
        // Defaults' states are those the page was first shown with: nothing changed.
        assert!(world.resource::<PauseView>().found.changed.is_empty());
        let texts: Vec<String> = world.query::<&Text>().iter(world).map(|t| t.0.clone()).collect();
        assert!(texts.iter().any(|t| t == "No changed settings."), "{texts:?}");
        // Both rows share one switch here (Lamp): both are listed, each under its place.
        crate::settings::flip(world, 0);
        app.update();
        let world = app.world_mut();
        let view = world.resource::<PauseView>();
        assert_eq!(view.found.changed, [RowId::Setting(0), RowId::Setting(1)]);
        assert_eq!(view.drawn.as_ref().unwrap().entries.len(), 4);
        assert_eq!(view.changed_logged.as_deref(), Some(&[RowId::Setting(0), RowId::Setting(1)][..]));
        assert_eq!(*world.resource::<Page>(), Page::Settings { selected: Some(RowId::Setting(0)) });
        crate::settings::flip(world, 0);
        app.update();
        assert!(app.world().resource::<PauseView>().found.changed.is_empty());
        assert_eq!(*app.world().resource::<Page>(), Page::Settings { selected: None });
    }

    #[derive(Resource, Default)]
    struct Torch(bool);
    #[derive(Resource, Default)]
    struct Fsr(bool);

    #[test]
    fn defaults_on_the_changed_list_put_back_only_its_rows_a_freed_one_too() {
        let mut app = App::new();
        app.insert_resource(Reach(0.5)).init_resource::<Torch>().init_resource::<Fsr>();
        // Listed before the row that disables it (the anti-aliasing under FSR).
        let slider = crate::settings::Slider { get: |w| w.resource::<Reach>().0, set: |w, v| w.resource_mut::<Reach>().0 = v, min: 0.1, step: 0.05, shown: None };
        add_setting(&mut app, Setting::slider("aa", "Test", "AA", "", slider));
        add_setting(&mut app, Setting { key: "fsr", group: "Test", label: "FSR", note: "", switches: "", get: |w| w.resource::<Fsr>().0, set: |w, on| w.resource_mut::<Fsr>().0 = on, slider: None });
        add_setting(&mut app, Setting { key: "torch", group: "Test", label: "Torch", note: "", switches: "", get: |w| w.resource::<Torch>().0, set: |w, on| w.resource_mut::<Torch>().0 = on, slider: None });
        crate::settings::set_disabled_status(&mut app, "aa", |w| w.resource::<Fsr>().0.then_some("Off (FSR active)"));
        app.init_resource::<PauseView>();
        {
            let world = app.world_mut();
            world.resource_mut::<Reach>().0 = 0.8;
            world.resource_mut::<Fsr>().0 = true;
            world.resource_mut::<Torch>().0 = true;
            // Defaults' states; the list holds the AA and FSR rows, not the torch's.
            let mut view = world.resource_mut::<PauseView>();
            view.defaults = Some(vec![0.5, 0.0, 0.0]);
            view.found.changed = vec![RowId::Setting(0), RowId::Setting(1)];
        }
        app.world_mut()
            .run_system_cached(|view: Res<PauseView>, settings: Res<Settings>, mut commands: Commands| changed_defaults(&view, settings.list(), &mut commands))
            .unwrap();
        let world = app.world();
        assert!(!world.resource::<Fsr>().0, "FSR back");
        assert_eq!(world.resource::<Reach>().0, 0.5, "the AA freed by it back too");
        assert!(world.resource::<Torch>().0, "a row not listed is left as it is");
    }

    /// An App running the pause menu's `choose` on the settings page over `rows` (key,
    /// group), the search open on `query` from Debug's first row, its keys `focus`.
    fn search_app(rows: &[(&'static str, &'static str)], query: &str, focus: SettingsFocus) -> App {
        let mut app = App::new();
        app.init_resource::<Lamp>().init_resource::<Time>().init_resource::<PlayClock>().init_resource::<PauseAid>();
        app.init_resource::<ButtonInput<KeyCode>>().init_resource::<ButtonInput<MouseButton>>();
        app.insert_resource(NextState::<PlayState>::default()).init_resource::<PageRequest>().init_resource::<Listening>().init_resource::<SettingsLayout>().init_resource::<crate::graphics_resume::GraphicsResume>();
        app.init_resource::<LoadList>().init_resource::<Assets<Image>>().init_resource::<crate::save::SaveConfig>().init_resource::<PauseStatus>().init_resource::<SearchKeys>();
        app.add_message::<AppExit>().add_message::<crate::ui_sounds::UiSound>().add_message::<crate::save::SaveRequest>().add_message::<crate::save::LoadRequest>().add_message::<MainMenuRequest>();
        for (key, group) in rows {
            add_setting(&mut app, Setting { key, group, label: key, note: "", switches: "", get: |w| w.resource::<Lamp>().0, set: |w, on| w.resource_mut::<Lamp>().0 = on, slider: None });
        }
        app.insert_resource(Paused(true)).insert_resource(Page::Settings { selected: None });
        let god = RowId::Setting(rows.iter().position(|r| r.0 == "god-mode").unwrap());
        let mut view = PauseView { scope: SettingsScope::Search, focus, ..default() };
        view.search = SearchState { query: query.into(), from: Some((SettingsScope::Tab("Debug"), Some(god), 0)), ..default() };
        app.insert_resource(view);
        app.add_systems(Update, choose);
        app
    }

    const SEARCHED: [(&str, &str); 3] = [("difficulty", "Gameplay"), ("menu-pause", "Gameplay"), ("god-mode", "Debug")];

    /// The page back on Debug's god-mode, the search closed.
    fn back_on_debug(app: &App) {
        let view = app.world().resource::<PauseView>();
        assert_eq!((view.scope, view.focus, view.search.from), (SettingsScope::Tab("Debug"), SettingsFocus::Rows, None));
        assert!(view.search.query.is_empty());
        assert_eq!(*app.world().resource::<Page>(), Page::Settings { selected: Some(RowId::Setting(2)) });
    }

    #[test]
    fn the_rails_back_closes_an_open_search_before_it_leaves_the_page() {
        let mut app = search_app(&SEARCHED, "pause", SettingsFocus::Rail(RailItem::Back));
        app.world_mut().resource_mut::<PauseAid>().actions = vec![(0.0, "pick".into())];
        app.update();
        back_on_debug(&app);
        // Again, with the search closed: Back leaves the settings page.
        app.world_mut().resource_mut::<PauseView>().focus = SettingsFocus::Rail(RailItem::Back);
        app.world_mut().resource_mut::<PauseAid>().actions = vec![(0.0, "pick".into())];
        app.update();
        assert_eq!(*app.world().resource::<Page>(), Page::Main);
    }

    #[test]
    fn text_then_esc_in_one_frame_closes_the_search_and_text_then_enter_goes_to_the_results() {
        let t = |s: &str| SearchKey::Text(s.to_string());
        // Typed, then Esc, in one frame: the search closes (and the page stays).
        let mut app = search_app(&SEARCHED, "pa", SettingsFocus::SearchText);
        app.world_mut().resource_mut::<SearchKeys>().0 = vec![t("u"), SearchKey::Escape, t("x")];
        app.update();
        back_on_debug(&app);
        // Typed, then Enter: the text kept, the keys on its results, the first one chosen.
        let mut app = search_app(&SEARCHED, "pa", SettingsFocus::SearchText);
        app.world_mut().resource_mut::<SearchKeys>().0 = vec![t("u"), t("se"), SearchKey::Enter];
        app.update();
        let view = app.world().resource::<PauseView>();
        assert_eq!((view.scope, view.focus, view.search.query.as_str()), (SettingsScope::Search, SettingsFocus::Rows, "pause"));
        assert_eq!(*app.world().resource::<Page>(), Page::Settings { selected: Some(RowId::Setting(1)) });
        // Enter alone on the results is the results' own: it flips the chosen switch.
        app.world_mut().resource_mut::<SearchKeys>().0 = vec![SearchKey::Enter];
        app.world_mut().resource_mut::<PauseAid>().actions = vec![(0.0, "pick".into())];
        app.update();
        app.update();
        assert!(app.world().resource::<Lamp>().0, "the switch was flipped");
        assert_eq!(app.world().resource::<PauseView>().focus, SettingsFocus::Rows);
    }

    #[test]
    fn a_controllers_command_in_the_frame_of_a_typed_letter_still_counts() {
        let t = |s: &str| SearchKey::Text(s.to_string());
        // A letter and a controller's B (Esc pressed, no text): the search closes.
        let mut app = search_app(&SEARCHED, "pa", SettingsFocus::SearchText);
        app.world_mut().resource_mut::<SearchKeys>().0 = vec![t("u")];
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Escape);
        app.update();
        back_on_debug(&app);
        // A letter and the aid's `back`: the same.
        let mut app = search_app(&SEARCHED, "pa", SettingsFocus::SearchText);
        app.world_mut().resource_mut::<SearchKeys>().0 = vec![t("u")];
        app.world_mut().resource_mut::<PauseAid>().actions = vec![(0.0, "back".into())];
        app.update();
        back_on_debug(&app);
        // Letters and a controller's A (Enter pressed, no Enter typed): the text kept, the
        // keys on the results, nothing acted on.
        let mut app = search_app(&SEARCHED, "pau", SettingsFocus::SearchText);
        app.world_mut().resource_mut::<SearchKeys>().0 = vec![t("se")];
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::Enter);
        app.update();
        let view = app.world().resource::<PauseView>();
        assert_eq!((view.scope, view.focus, view.search.query.as_str()), (SettingsScope::Search, SettingsFocus::Rows, "pause"));
        assert!(!app.world().resource::<Lamp>().0);
    }

    #[test]
    fn the_columns_settings_entry_closes_an_open_search_and_takes_its_keys() {
        // Typing N in the frame the column's Settings entry is clicked: back to the column,
        // the search closed, and N saves nothing.
        let mut app = search_app(&SEARCHED, "pa", SettingsFocus::SearchText);
        app.world_mut().resource_mut::<SearchKeys>().0 = vec![SearchKey::Text("n".into())];
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyN);
        let settings = entries(false).iter().position(|e| *e == Entry::Settings).unwrap();
        app.world_mut().spawn((Button, PauseButton::Column(settings), PointerState::Pressed));
        app.update();
        assert_eq!(*app.world().resource::<Page>(), Page::Main);
        let view = app.world().resource::<PauseView>();
        assert_eq!((view.search.from, view.search.query.as_str(), view.scope), (None, "", SettingsScope::Tab("Debug")));
        assert!(app.world().resource::<PauseStatus>().line.is_empty(), "no save was asked: {:?}", app.world().resource::<PauseStatus>().line);
        assert_eq!(view.confirm, None);
        assert!(!app.world().resource::<ButtonInput<KeyCode>>().just_pressed(KeyCode::KeyN), "the systems after it do not see N");
    }

    #[test]
    fn a_result_of_the_device_not_in_use_names_what_its_input_device_row_stands_for() {
        use crate::bindings::{Bindings, Control, InputDevice};
        let mut app = settings_app(&[("difficulty", "Gameplay")], Page::Settings { selected: None });
        let mut b = Bindings::game_defaults();
        b.rebind_pad(Control::Jump, crate::pad::PadInput::Button(GamepadButton::RightTrigger));
        app.insert_resource(b);
        app.world_mut().resource_mut::<PauseView>().scope = SettingsScope::Changed;
        app.update();
        let world = app.world_mut();
        let d = world.resource::<PauseView>().drawn.clone().unwrap();
        // Jump's and V.A.T.S.'s controller buttons changed, the keyboard in use: one Input
        // Device row, which names them.
        assert_eq!(d.entries.iter().filter(|e| matches!(e, ViewEntry::Row(_))).count(), 1);
        assert_eq!(*world.resource::<Page>(), Page::Settings { selected: Some(RowId::Control(ControlRow::Device)) });
        let note = world.query_filtered::<&Text, With<SwitchNote>>().single(world).unwrap().0.clone();
        assert!(note.starts_with("Stands for: Jump (Controller), V.A.T.S. (Controller)."), "{note}");
        let changed = &world.resource::<PauseView>().found.changed;
        assert_eq!(changed, &[RowId::Control(ControlRow::Binding(Control::Jump, InputDevice::Controller)), RowId::Control(ControlRow::Binding(Control::Vats, InputDevice::Controller))]);
    }

    #[test]
    fn the_settings_page_opens_where_it_was_left_and_from_the_search_where_it_came_from() {
        let mut view = PauseView { scope: SettingsScope::Tab("Debug"), scroll: 3, ..default() };
        remember(&mut view, Some(RowId::Setting(2)));
        let mut page = Page::Main;
        view.scope = SettingsScope::default();
        view.scroll = 0;
        open_settings(&mut page, &mut view, Some("Graphics"));
        assert_eq!((page, view.scope, view.scroll), (Page::Settings { selected: Some(RowId::Setting(2)) }, SettingsScope::Tab("Debug"), 3));
        // Left from the search: the tab, row and line it came from; its text not kept.
        view.scope = SettingsScope::Search;
        view.search = SearchState { query: "reverb".into(), from: Some((SettingsScope::Tab("Audio"), Some(RowId::Setting(1)), 0)), ..default() };
        remember(&mut view, None);
        open_settings(&mut page, &mut view, None);
        assert_eq!((page, view.scope, view.scroll), (Page::Settings { selected: Some(RowId::Setting(1)) }, SettingsScope::Tab("Audio"), 0));
        assert_eq!(view.search, SearchState::default());
        // The first time in a run: the INI's tab, else the first.
        let mut fresh = PauseView::default();
        open_settings(&mut page, &mut fresh, Some("Graphics"));
        assert_eq!((page, fresh.scope), (Page::Settings { selected: None }, SettingsScope::Tab("Graphics")));
        let mut fresh = PauseView::default();
        open_settings(&mut page, &mut fresh, None);
        assert_eq!(fresh.scope, SettingsScope::default());
    }

    #[test]
    fn back_out_and_settings_again_shows_the_same_tab_and_row() {
        let mut app = search_app(&SEARCHED, "", SettingsFocus::Rows);
        let god = RowId::Setting(2);
        {
            let mut view = app.world_mut().resource_mut::<PauseView>();
            view.scope = SettingsScope::Tab("Debug");
            view.search = SearchState::default();
        }
        *app.world_mut().resource_mut::<Page>() = Page::Settings { selected: Some(god) };
        app.update();
        app.world_mut().resource_mut::<PauseAid>().actions = vec![(0.0, "back".into())];
        app.update();
        assert_eq!(*app.world().resource::<Page>(), Page::Main);
        app.world_mut().resource_mut::<PauseView>().scope = SettingsScope::default();
        app.world_mut().resource_mut::<PauseAid>().actions = vec![(0.0, "settings".into())];
        app.update();
        assert_eq!(*app.world().resource::<Page>(), Page::Settings { selected: Some(god) });
        assert_eq!(app.world().resource::<PauseView>().scope, SettingsScope::Tab("Debug"));
    }

    #[test]
    fn the_tab_is_kept_in_the_ini_and_an_unknown_one_opens_the_first() {
        let dir = std::env::temp_dir().join(format!("nvf-settings-tab-{}", std::process::id()));
        let path = dir.join("NVForever.ini");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "[Settings]\ngrass=0\n").unwrap();
        let mut store = SettingsTabStore { path: Some(path.clone()), tab: None };
        store.keep("Graphics");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("grass=0") && text.contains("[UI]") && text.contains("SettingsTab=Graphics"), "{text}");
        // Read back at the next start.
        let mut app = App::new();
        app.insert_resource(crate::settings::SettingsFile(Some(path.clone()))).init_resource::<SettingsTabStore>();
        app.world_mut().run_system_cached(read_settings_tab).unwrap();
        assert_eq!(app.world().resource::<SettingsTabStore>().tab.as_deref(), Some("Graphics"));
        // No INI (an unattended run in the player's home): nothing read, nothing written.
        let mut none = SettingsTabStore::default();
        none.keep("Debug");
        assert_eq!(none.tab.as_deref(), Some("Debug"));
        let _ = std::fs::remove_dir_all(&dir);
        // The settings page opens on the kept tab; an unknown name on the first.
        let rows = [("difficulty", "Gameplay"), ("grade", "Display"), ("god-mode", "Debug")];
        for (kept, shown) in [("Graphics", "Graphics"), ("Nope", "Gameplay")] {
            let mut app = search_app(&rows, "", SettingsFocus::Rows);
            *app.world_mut().resource_mut::<Page>() = Page::Main;
            app.insert_resource(PauseView::default()).insert_resource(SettingsTabStore { path: None, tab: Some(kept.into()) });
            app.world_mut().resource_mut::<PauseAid>().actions = vec![(0.0, "settings".into())];
            app.update();
            assert_eq!(app.world().resource::<PauseView>().scope, SettingsScope::Tab(shown), "{kept}");
        }
    }

    #[test]
    fn a_rail_line_under_the_pointer_is_boxed_unfilled_and_bright() {
        let mut app = settings_app(&[("difficulty", "Gameplay"), ("god-mode", "Debug")], Page::Settings { selected: None });
        app.world_mut().resource_mut::<PauseView>().rail_hover = Some(RailItem::Tab("Debug"));
        app.update();
        let world = app.world_mut();
        let looks: Vec<(PauseButton, Color, Color)> = world.query::<(&PauseButton, &BorderColor, &BackgroundColor)>().iter(world).map(|(b, l, f)| (*b, l.top, f.0)).collect();
        let look = |item| looks.iter().find(|(b, ..)| *b == PauseButton::Rail(item)).map(|(_, l, f)| (*l != Color::NONE, *f != Color::NONE)).unwrap();
        assert_eq!(look(RailItem::Tab("Debug")), (true, false), "the line, not filled");
        assert_eq!(look(RailItem::Tab("Audio")), (false, false), "another tab: no box");
        let texts: Vec<(String, Color)> = world.query::<(&Text, &TextColor)>().iter(world).map(|(t, c)| (t.0.clone(), c.0)).collect();
        assert!(texts.contains(&("Debug".to_string(), color(Shade::Bright))), "{texts:?}");
        assert!(texts.contains(&("Audio".to_string(), color(Shade::Dim))));
        // The keys' line on the rail keeps its filled box.
        world.resource_mut::<PauseView>().focus = SettingsFocus::Rail(RailItem::Tab("Debug"));
        app.update();
        let world = app.world_mut();
        let debug = world.query::<(&PauseButton, &BorderColor, &BackgroundColor)>().iter(world).find(|(b, ..)| **b == PauseButton::Rail(RailItem::Tab("Debug"))).map(|(_, l, f)| (l.top != Color::NONE, f.0 != Color::NONE)).unwrap();
        assert_eq!(debug, (true, true));
    }

    #[test]
    fn the_pointer_coming_onto_a_rail_line_sounds_once() {
        use crate::ui_sounds::{UiEvent, UiSound};
        let mut app = search_app(&SEARCHED, "", SettingsFocus::Rows);
        app.world_mut().resource_mut::<PauseView>().scope = SettingsScope::Tab("Gameplay");
        app.world_mut().resource_mut::<PauseView>().search = SearchState::default();
        let e = app.world_mut().spawn((Button, PauseButton::Rail(RailItem::Tab("Debug")), PointerState::Hovered)).id();
        let mut reader = bevy::ecs::system::SystemState::<MessageReader<UiSound>>::new(app.world_mut());
        let mut focus_sounds = |app: &mut App| reader.get_mut(app.world_mut()).unwrap().read().filter(|s| s.event == UiEvent::Focus).count();
        app.update();
        assert_eq!(app.world().resource::<PauseView>().rail_hover, Some(RailItem::Tab("Debug")));
        assert_eq!(focus_sounds(&mut app), 1);
        // Still there (or drawn again under it): no sound again.
        app.update();
        assert_eq!(focus_sounds(&mut app), 0);
        // Off it: no hover; the tab shown is unchanged.
        *app.world_mut().get_mut::<PointerState>(e).unwrap() = PointerState::None;
        app.update();
        let view = app.world().resource::<PauseView>();
        assert_eq!((view.rail_hover, view.scope), (None, SettingsScope::Tab("Gameplay")));
    }

    #[test]
    fn the_mods_entry_stands_before_quit_only_when_a_mod_added_one() {
        let labels = |mods| entries(mods).iter().map(|e| e.label()).collect::<Vec<_>>();
        assert_eq!(labels(false), ["Continue", "Save", "Load", "Settings", "Quit"]);
        assert_eq!(labels(true), ["Continue", "Save", "Load", "Settings", "Mods", "Quit"]);
        assert!(Page::Mods.opened_by("Mods"));
        assert!(!Page::Mods.opened_by("Settings"));
        assert_eq!(Page::Mods.settings_row(), None);
        assert!(mods_open(&Paused(true), &Page::Mods));
        assert!(!mods_open(&Paused(false), &Page::Mods));
        assert!(!mods_open(&Paused(true), &Page::Main));
    }

    #[test]
    fn a_page_is_closed_by_its_own_entry() {
        // The pause menu's labels and the main menu's ("Load Game").
        assert!(Page::Settings { selected: Some(RowId::Setting(3)) }.opened_by("Settings"));
        assert!(Page::Load { selected: 0 }.opened_by("Load"));
        assert!(Page::Load { selected: 0 }.opened_by("Load Game"));
        assert!(!Page::Load { selected: 0 }.opened_by("Settings"));
        assert!(!Page::Settings { selected: None }.opened_by("Load Game"));
        assert!(!Page::Main.opened_by("Settings"));
    }

    #[test]
    fn the_keys_go_round_the_rows() {
        assert_eq!(step_row(0, 1, 3), 1);
        assert_eq!(step_row(2, 1, 3), 0);
        assert_eq!(step_row(0, -1, 3), 2);
        assert_eq!(step_row(7, -1, 3), 1);
        assert_eq!(step_row(0, 1, 0), 0);
    }

    /// #2377: the Controls tab's own notes, on either device and while a control waits for its
    /// key, are within the notes' budget (`crate::settings::note_over_budget`), and a row
    /// standing for many names three of them.
    #[test]
    fn the_controls_notes_are_within_the_budget() {
        use crate::bindings::{InputDevice, GAME_CONTROLS};
        assert_eq!(crate::settings::note_over_budget(AUTOMATIC_INPUT_NOTE), None);
        for controller in [false, true] {
            for device in [InputDevice::KeyboardMouse, InputDevice::Controller] {
                let mut rows = vec![ControlRow::Device, ControlRow::AutomaticInput, ControlRow::Invert, ControlRow::Sensitivity(device), ControlRow::Vertical(device), ControlRow::Rumble];
                rows.extend(GAME_CONTROLS.iter().map(|(c, _, _)| ControlRow::Binding(*c, device)));
                for listening in [None, Some(GAME_CONTROLS[0].0)] {
                    let d = Drawn { device: ("", controller), listening, ..default() };
                    for c in &rows {
                        let note = row_note(&[], RowId::Control(*c), &d);
                        assert!(!note.is_empty(), "{c:?}");
                        assert_eq!(crate::settings::note_over_budget(&note), None, "{c:?} ({controller}): {note}");
                    }
                }
            }
        }
        let id = RowId::Control(ControlRow::Rumble);
        let names: Vec<String> = ["A", "B", "C", "D", "E"].map(String::from).to_vec();
        let d = Drawn { stand_ins: vec![(id, names)], ..default() };
        assert!(note_of(&[], id, &d).starts_with("Stands for: A, B, C and 2 more. "), "{}", note_of(&[], id, &d));
    }

    /// #2377: a note line wider than the page (a word longer than the line, which wrapping
    /// leaves whole) is cut to the page's width with the mark.
    #[test]
    fn a_note_wider_than_the_page_is_cut() {
        let font = Faces::plain().small.font;
        let width = 300.0;
        let note = format!("Short words then {} and more words after it.", "x".repeat(80));
        let lines = note_wrapped(&font, &note, width);
        assert!(lines.len() >= 2 && lines.iter().all(|l| l.width <= width), "{:?}", lines.iter().map(|l| l.width).collect::<Vec<_>>());
        assert!(lines.iter().any(|l| l.text().ends_with(CUT_MARK)));
        assert_eq!(note_wrapped(&font, "Two short words.", width).len(), 1);
    }

    /// #2377: text cut to fit is cut at its measured width, with the mark, never into
    /// nothing while the mark fits; text that fits is left whole.
    #[test]
    fn text_that_does_not_fit_is_cut_with_a_mark() {
        let chars = |t: &str| t.chars().count() as f32;
        assert_eq!(fit_text(chars, "Bloom", 5.0), "Bloom");
        assert_eq!(fit_text(chars, "Anisotropic", 6.0), "Ani...");
        assert_eq!(fit_text(chars, "ab cdef", 5.0), "ab...", "no space before the mark");
        assert_eq!(fit_text(chars, "Größenänderung", 7.0), "Größ...", "by characters, not bytes");
        assert_eq!(fit_text(chars, "Anisotropic", 3.0), "...");
        assert_eq!(fit_text(chars, "Anisotropic", 2.0), "");
        assert_eq!(fit_text(chars, "", 0.0), "");
        let wide = |t: &str| t.chars().map(|c| if c == 'W' { 3.0 } else { 1.0 }).sum::<f32>();
        assert_eq!(fit_text(wide, "WWWW", 7.0), "W...", "measured, not counted");
    }

    /// #2377: no text runs into its neighbour: at every view width and rail, for every kind
    /// of value and long words, the label ends LABEL_GAP before the value, the value stays in
    /// its row, words drawn stay in their room, a slider's meter and arrows keep their order;
    /// a row of the width before #2377 draws a usual value where it did.
    #[test]
    fn a_rows_label_and_value_never_overlap() {
        let font = Faces::plain().entry.font;
        let width = |t: &str| font.width(t);
        let long = "An extremely long translated label that cannot fit in any settings row at all";
        let values = ["ON", "Off (upscaling active)", "Controlled by another setting entirely, with more words", "Performance", "Ultra Performance mode at a very long name", "50%", "Right Bracket, Middle Mouse", "Left Shift, Right Bracket, Middle Mouse, Mouse Wheel Down"];
        for view_w in VIEW_WIDTHS {
            for tabs_w in [SUB_WIDTH, 220.0] {
                let lb = ListBox::of(view_w, tabs_w);
                let x = lb.row();
                for value in values {
                    let vw = width(value);
                    for kind in [ValueKind::Centred(vw), ValueKind::Slider(vw), ValueKind::List(vw), ValueKind::Binding(vw)] {
                        let p = row_parts(x, kind);
                        let at = format!("{view_w}/{tabs_w} {kind:?} {value:?}");
                        assert!(p.label_room() >= LABEL_MIN.min(x.value_left() - 10.0 - LABEL_GAP) - 0.01, "{at}: label room {}", p.label_room());
                        assert!(p.label.1 + LABEL_GAP <= p.value.0 + 0.01, "{at}: label to {}, value from {}", p.label.1, p.value.0);
                        assert!(p.value.1 <= lb.width + 0.01 && p.text.0 >= p.value.0 - 0.01 && p.text.1 <= p.value.1 + 0.01, "{at}: {p:?}");
                        let (l, v) = fit_row(width, &p, long, value);
                        assert!(width(&l) <= p.label_room() + 0.01 && p.label.0 + width(&l) + LABEL_GAP <= p.value.0 + 0.01, "{at}: label {l:?}");
                        let tx = p.text_x(width(&v));
                        assert!(width(&v) <= p.text_room() + 0.01 && tx >= p.text.0 - 0.01 && tx + width(&v) <= p.text.1 + 0.01 && tx + width(&v) <= lb.width, "{at}: value {v:?} at {tx}");
                        if let ValueKind::Slider(_) = kind {
                            let (ml, mw) = p.meter;
                            assert!(p.value.0 + METER_ARROW.x <= ml && ml + mw < p.right_arrow && p.right_arrow + METER_ARROW.x <= p.text.0 && mw >= METER_MIN, "{at}: {p:?}");
                        }
                    }
                }
            }
        }
        // A row 820 wide, as before #2377: usual values where they were.
        let x = RowX(ROW_BASE);
        assert_eq!(row_parts(x, ValueKind::Centred(34.0)).text_x(34.0), VALUE_AT - 17.0);
        let p = row_parts(x, ValueKind::Slider(51.0));
        assert_eq!((p.text_x(51.0), p.right_arrow, p.meter), (PERCENT_AT - 25.5, METER_ARROWS[1], (METER_LEFT, METER_WIDTH)));
        assert_eq!(row_parts(x, ValueKind::List(100.0)).text_x(100.0), LIST_LEFT + (LIST_WIDTH - 30.0 - 100.0) / 2.0);
        assert_eq!(row_parts(x, ValueKind::Binding(60.0)).text.0, VALUE_AT - 30.0);
        assert_eq!(row_parts(x, ValueKind::Centred(34.0)).label, (10.0, LIST_LEFT - LABEL_GAP));
        // Wide words: a slider's meter gives room ("Performance" at 17 units a letter), a
        // reason takes room from the label, both within their row.
        let p = row_parts(x, ValueKind::Slider(187.0));
        assert!(p.meter.1 < METER_WIDTH && p.text_room() >= 187.0, "{p:?}");
        let p = row_parts(x, ValueKind::Centred(500.0));
        assert!(p.value.0 < LIST_LEFT && p.label_room() >= LABEL_MIN && p.text_room() >= 500.0, "{p:?}");
    }

    #[test]
    fn the_window_moves_only_as_far_as_keeps_the_choice_in_view() {
        // A list of rows without headings (the Controls tab's): as before #1953.
        let e: Vec<ViewEntry> = (0..20).map(|i| ViewEntry::Row(RowId::Setting(i))).collect();
        for lines in [9, window_lines()] {
            let follow = |first, sel| crate::settings_view::follow(&e, first, Some(sel), lines);
            // Down past the window's foot: it moves one row; back up inside it: it stays.
            assert_eq!(follow(0, lines - 1), 0);
            assert_eq!(follow(0, lines), 1);
            assert_eq!(follow(1, 5), 1);
            assert_eq!(follow(5, 2), 2);
            // Never past the last full window, nor anywhere when all rows fit.
            assert_eq!(follow(15, 19), 20 - lines);
            assert_eq!(crate::settings_view::follow(&e[..6], 3, Some(2), lines), 0);
        }
    }

    #[test]
    fn the_scroll_bar_shows_only_when_rows_are_hidden_and_its_marker_follows_the_list() {
        for lines in [9, window_lines()] {
            let w = settings_window(lines);
            assert_eq!(w.marker(lines, 0), None);
            assert_eq!(w.marker(4, 0), None);
            let (top, h) = w.marker(30, 0).unwrap();
            assert_eq!(top, 0.0);
            assert!((h - w.track() * lines as f32 / 30.0).abs() < 1e-3);
            let last = 30 - lines;
            let (end, _) = w.marker(30, last).unwrap();
            assert!((end + h - w.track()).abs() < 1e-3);
            // A press at the track's ends scrolls to the list's ends, its middle halfway.
            assert_eq!(w.first_at(30, 0.0), 0);
            assert_eq!(w.first_at(30, 1.0), last);
            assert_eq!(w.first_at(30, 0.5), (last as f32 / 2.0).round() as usize);
            assert_eq!(w.first_at(5, 0.7), 0);
        }
        // A window of a line or two: the track is shorter than the marker's least height
        // (or none); the marker fills it and a press still picks a place in the list.
        for lines in [1, 2] {
            let w = settings_window(lines);
            let (top, h) = w.marker(30, 10).unwrap();
            assert!(h > 0.0 && top >= 0.0 && top + h <= w.track() + 1e-3, "{lines}: {top} {h} in {}", w.track());
            if lines == 1 {
                assert!(top == 0.0 && (h - w.track()).abs() < 1e-3, "one line: a 20-unit track, the marker fills it");
            }
            assert_eq!(w.first_at(30, 0.0), 0);
            assert_eq!(w.first_at(30, 1.0), 30 - lines);
            assert!(w.first_at(30, 0.5) <= 30 - lines);
        }
        assert_eq!(settings_window(0).shown, 1, "never a window of no lines");
        // The load page's bar shows past eight saves and reaches the last eight.
        assert_eq!(LOAD_WINDOW.marker(8, 0), None);
        assert!(LOAD_WINDOW.marker(21, 0).is_some());
        assert_eq!(LOAD_WINDOW.first_at(21, 1.0), 13);
    }

    #[test]
    fn the_pointer_takes_only_rows_in_the_window() {
        // Rows 5-12 shown: a row drawn before the window moved (4 or 13) is not taken.
        assert!(LOAD_WINDOW.holds(5, 5) && LOAD_WINDOW.holds(5, 12));
        assert!(!LOAD_WINDOW.holds(5, 4) && !LOAD_WINDOW.holds(5, 13));
        let w = settings_window(window_lines());
        assert!(w.holds(0, w.shown - 1) && !w.holds(0, w.shown));
    }

    #[test]
    fn a_new_save_takes_the_next_number() {
        assert_eq!(next_save_name([]), "Save 1");
        assert_eq!(next_save_name(["quicksave", "Save 2", "Save 10", "Save x", "autosave"]), "Save 11");
    }

    #[test]
    fn the_load_page_says_times_plainly() {
        assert_eq!(play_time(59.0), "0 min");
        assert_eq!(play_time(3.0 * 3600.0 + 7.0 * 60.0), "3 h 07 min");
        assert_eq!(saved_ago(1000, 1030), "just now");
        assert_eq!(saved_ago(1000, 1000 + 5 * 60), "5 min ago");
        assert_eq!(saved_ago(1000, 1000 + 2 * 86400), "2 days ago");
        assert_eq!(saved_ago(2000, 1000), "just now");
    }

    #[test]
    fn the_load_page_lists_the_saves_and_shows_the_chosen_one() {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>().init_resource::<crate::pad::PadGlyphs>();
        let mut list = LoadList::default();
        for (name, t) in [("Save 2", 20), ("autosave", 10)] {
            let mut h = SaveHeader::new(if name == "autosave" { SaveKind::Auto } else { SaveKind::Manual });
            h.saved_unix = t;
            h.player_name = "Courier".into();
            h.location = "Goodsprings".into();
            list.saves.push((PathBuf::from(format!("{name}.nvfsave")), h));
        }
        app.insert_resource(list);
        let drawn = Drawn { page: Page::Load { selected: 1 }, column: entries(false).iter().map(|e| (e.label(), true)).collect(), device: ("Keyboard and Mouse", false), rumble: true, sensitivity: 1.0, view_w: VIEW_W as u32, ..default() };
        app.world_mut().run_system_cached_with(|In(d): In<Drawn>, mut commands: Commands, list: Res<LoadList>, glyphs: Res<crate::pad::PadGlyphs>| spawn_page(&mut commands, &d, &PauseLook::plain(), &[], &list, &glyphs), drawn).unwrap();
        let world = app.world_mut();
        let mut rows = world.query::<(&PauseButton, &BorderColor)>();
        let rows: Vec<(PauseButton, bool)> = rows.iter(world).filter(|(b, _)| matches!(b, PauseButton::LoadRow(_))).map(|(b, c)| (*b, c.top != Color::NONE)).collect();
        assert_eq!(rows.len(), 2);
        // The chosen row has the box round it, and only it.
        assert!(rows.contains(&(PauseButton::LoadRow(1), true)) && rows.contains(&(PauseButton::LoadRow(0), false)));
        // Load is lit in the column, the others dimmed.
        let mut texts = world.query::<(&Text, &TextColor)>();
        let all: Vec<(String, Color)> = texts.iter(world).map(|(t, c)| (t.0.clone(), c.0)).collect();
        assert!(all.contains(&("Load".to_string(), color(Shade::Bright))) && all.contains(&("Save".to_string(), color(Shade::Dim))), "{all:?}");
        let mut details = world.query_filtered::<&Text, With<DetailLine>>();
        let details: Vec<String> = details.iter(world).map(|t| t.0.clone()).collect();
        assert!(details.iter().any(|t| t == "Courier, level 1"), "{details:?}");
        // The stand-in font keeps the spaces between words (#2298).
        assert!(all.iter().any(|t| t.0 == "No picture"), "{all:?}");
    }

    #[derive(Resource, Default)]
    struct Lamp(bool);

    #[test]
    fn the_settings_page_shows_each_switch_as_the_game_has_it() {
        let mut app = App::new();
        app.init_resource::<Lamp>().init_resource::<Paused>().insert_resource(Page::Settings { selected: Some(RowId::Setting(0)) });
        add_setting(&mut app, Setting { key: "lamp", group: "Test", label: "Lamp", note: "A lamp.", switches: "", get: |w| w.resource::<Lamp>().0, set: |w, on| w.resource_mut::<Lamp>().0 = on, slider: None });
        app.init_resource::<PauseView>().init_resource::<LoadList>().init_resource::<PauseStatus>().init_resource::<crate::pad::PadGlyphs>().insert_resource(PauseLook::plain());
        app.world_mut().resource_mut::<Paused>().0 = true;
        app.add_systems(Update, draw);
        let state = |app: &mut App| {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<(&SwitchState, &Text)>();
            q.single(world).unwrap().1 .0.clone()
        };
        assert_eq!(state(&mut app), "OFF");
        crate::settings::flip(app.world_mut(), 0);
        assert_eq!(state(&mut app), "ON");
        // Defaults go back to the state the page was first shown with.
        assert_eq!(app.world().resource::<PauseView>().defaults, Some(vec![0.0]));
        let world = app.world_mut();
        let mut note = world.query_filtered::<&Text, With<SwitchNote>>();
        assert_eq!(note.single(world).unwrap().0, "A lamp.");
    }

    #[derive(Resource)]
    struct Reach(f32);

    #[test]
    fn a_disabled_slider_shows_its_reason_without_controls_and_restores_its_saved_value() {
        let mut app = App::new();
        app.insert_resource(Reach(0.5)).insert_resource(Lamp(true)).init_resource::<Paused>().insert_resource(Page::Settings { selected: Some(RowId::Setting(0)) });
        let slider = crate::settings::Slider { get: |w| w.resource::<Reach>().0, set: |w, v| w.resource_mut::<Reach>().0 = v, min: 0.1, step: 0.05, shown: None };
        add_setting(&mut app, Setting::slider("reach", "Test", "Reach", "How far.", slider));
        crate::settings::set_disabled_status(&mut app, "reach", |w| w.resource::<Lamp>().0.then_some("Off (FSR active)"));
        app.init_resource::<PauseView>().init_resource::<LoadList>().init_resource::<PauseStatus>().init_resource::<crate::pad::PadGlyphs>().insert_resource(PauseLook::plain());
        app.world_mut().resource_mut::<Paused>().0 = true;
        app.add_systems(Update, draw);
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<(&SwitchState, &Text)>().single(world).unwrap().1.0, "Off (FSR active)");
        let buttons: Vec<PauseButton> = world.query::<&PauseButton>().iter(world).copied().collect();
        assert!(!buttons.iter().any(|b| matches!(b, PauseButton::Meter(RowId::Setting(0)) | PauseButton::SliderStep(RowId::Setting(0), _))), "disabled sliders have no clickable adjustment controls");
        assert!(view_disabled(world.resource::<PauseView>(), 0));
        assert!(crate::settings::step_slider(world, 0, 1).is_none());
        assert_eq!(world.resource::<Reach>().0, 0.5);

        world.resource_mut::<Lamp>().0 = false;
        app.update();
        let world = app.world_mut();
        assert_eq!(world.query::<(&SwitchState, &Text)>().single(world).unwrap().1.0, "50%");
        assert!(!view_disabled(world.resource::<PauseView>(), 0));
        assert!(world.query::<&PauseButton>().iter(world).any(|b| *b == PauseButton::Meter(RowId::Setting(0))));
        assert_eq!(crate::settings::step_slider(world, 0, 1), Some(0.55));
    }

    #[test]
    fn a_slider_row_shows_its_value_as_a_meter_and_a_percentage() {
        let mut app = App::new();
        app.insert_resource(Reach(0.5)).init_resource::<Paused>().insert_resource(Page::Settings { selected: Some(RowId::Setting(0)) });
        let slider = crate::settings::Slider { get: |w| w.resource::<Reach>().0, set: |w, v| w.resource_mut::<Reach>().0 = v, min: 0.1, step: 0.05, shown: None };
        add_setting(&mut app, Setting::slider("reach", "Test", "Reach", "How far.", slider));
        app.init_resource::<PauseView>().init_resource::<LoadList>().init_resource::<PauseStatus>().init_resource::<crate::pad::PadGlyphs>().insert_resource(PauseLook::plain());
        app.world_mut().resource_mut::<Paused>().0 = true;
        app.add_systems(Update, draw);
        let shown = |app: &mut App| {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<(&SwitchState, &Text)>();
            let value = q.single(world).unwrap().1 .0.clone();
            let mut b = world.query::<&PauseButton>();
            let buttons: Vec<PauseButton> = b.iter(world).copied().collect();
            let r = RowId::Setting(0);
            assert!(buttons.contains(&PauseButton::Meter(r)) && buttons.contains(&PauseButton::SliderStep(r, -1)) && buttons.contains(&PauseButton::SliderStep(r, 1)), "{buttons:?}");
            value
        };
        assert_eq!(shown(&mut app), "50%");
        crate::settings::step_slider(app.world_mut(), 0, -2);
        assert_eq!(shown(&mut app), "40%");
        assert_eq!(app.world().resource::<PauseView>().defaults, Some(vec![0.5]));
        assert_eq!(meter_ticks(1.0, METER_WIDTH), 30);
        assert_eq!(meter_ticks(0.5, METER_WIDTH), 15);
        assert_eq!(meter_ticks(0.0, METER_WIDTH), 0);
    }

    #[test]
    fn the_controls_tab_shows_each_key_and_waits_for_a_new_one() {
        use crate::bindings::{Bindings, Control, Input, InputDevice};
        let control = |c| RowId::Control(c);
        let jump = control(ControlRow::Binding(Control::Jump, InputDevice::KeyboardMouse));
        let mut app = App::new();
        app.init_resource::<Paused>().init_resource::<Settings>().init_resource::<Bindings>().init_resource::<Listening>().insert_resource(Page::Settings { selected: Some(jump) });
        app.init_resource::<PauseView>().init_resource::<LoadList>().init_resource::<PauseStatus>().init_resource::<crate::pad::PadGlyphs>().insert_resource(PauseLook::plain());
        app.world_mut().resource_mut::<Paused>().0 = true;
        app.add_systems(Update, draw);
        // The value drawn on the row that is `which`, choosing it first.
        let value_of = |app: &mut App, which: fn(&RowId) -> bool| {
            app.update();
            let world = app.world_mut();
            let mut q = world.query::<(&SwitchState, &Text)>();
            q.iter(world).find(|(s, _)| which(&s.0)).map(|(_, t)| t.0.clone())
        };
        let is_jump = |r: &RowId| matches!(r, RowId::Control(ControlRow::Binding(Control::Jump, _)));
        assert_eq!(value_of(&mut app, is_jump).as_deref(), Some("Space"));
        assert_eq!(app.world().resource::<PauseView>().scope, SettingsScope::Tab(CONTROLS_TAB));
        crate::bindings::rebind(app.world_mut(), Control::Jump, Input::Key(KeyCode::KeyJ));
        assert_eq!(value_of(&mut app, is_jump).as_deref(), Some("J"));
        app.world_mut().resource_mut::<Listening>().0 = Some((Control::Jump, InputDevice::KeyboardMouse));
        assert_eq!(value_of(&mut app, is_jump).as_deref(), Some(WAITING));
        app.world_mut().resource_mut::<Listening>().0 = None;
        assert!(Page::Settings { selected: Some(jump) }.opened_by("Settings"));
        // With a controller the rows show its buttons: Jump is Y [hypothesis, `crate::pad`];
        // the choice moves to the control's controller row.
        crate::bindings::set_device(app.world_mut(), InputDevice::Controller);
        assert_eq!(value_of(&mut app, is_jump).as_deref(), Some("Y"));
        assert_eq!(*app.world().resource::<Page>(), Page::Settings { selected: Some(control(ControlRow::Binding(Control::Jump, InputDevice::Controller))) });
        let choose_row = |app: &mut App, c: ControlRow| *app.world_mut().resource_mut::<Page>() = Page::Settings { selected: Some(RowId::Control(c)) };
        choose_row(&mut app, ControlRow::AutomaticInput);
        assert_eq!(value_of(&mut app, |r| *r == RowId::Control(ControlRow::AutomaticInput)).as_deref(), Some("ON"));
        crate::bindings::set_automatic_input_switching(app.world_mut(), false);
        assert_eq!(value_of(&mut app, |r| *r == RowId::Control(ControlRow::AutomaticInput)).as_deref(), Some("OFF"));
        choose_row(&mut app, ControlRow::Device);
        assert_eq!(value_of(&mut app, |r| *r == RowId::Control(ControlRow::Device)).as_deref(), Some("Controller"));
        // With a controller Rumble comes after the sensitivity, on by default.
        let is_rumble = |r: &RowId| *r == RowId::Control(ControlRow::Rumble);
        choose_row(&mut app, ControlRow::Rumble);
        assert_eq!(value_of(&mut app, is_rumble).as_deref(), Some("ON"));
        crate::bindings::set_rumble(app.world_mut(), false);
        assert_eq!(value_of(&mut app, is_rumble).as_deref(), Some("OFF"));
        crate::bindings::set_device(app.world_mut(), InputDevice::KeyboardMouse);
        assert_eq!(value_of(&mut app, is_rumble), None, "no Rumble row with keyboard and mouse");
        assert_eq!(*app.world().resource::<Page>(), Page::Settings { selected: Some(control(ControlRow::Device)) }, "its choice goes to the Input Device row");
        // Mouse Sensitivity, the game's at first, kept apart from the stick's.
        let is_sensitivity = |r: &RowId| matches!(r, RowId::Control(ControlRow::Sensitivity(_)));
        choose_row(&mut app, ControlRow::Sensitivity(InputDevice::KeyboardMouse));
        assert_eq!(value_of(&mut app, is_sensitivity).as_deref(), Some("100%"));
        crate::bindings::set_sensitivity(app.world_mut(), 1.5);
        assert_eq!(value_of(&mut app, is_sensitivity).as_deref(), Some("150%"));
        crate::bindings::set_device(app.world_mut(), InputDevice::Controller);
        assert_eq!(value_of(&mut app, is_sensitivity).as_deref(), Some("100%"), "the stick's own");
        crate::bindings::set_sensitivity(app.world_mut(), 0.5);
        assert_eq!(value_of(&mut app, is_sensitivity).as_deref(), Some("50%"));
        crate::bindings::set_device(app.world_mut(), InputDevice::KeyboardMouse);
        assert_eq!(value_of(&mut app, is_sensitivity).as_deref(), Some("150%"), "the mouse's kept");
        let mouse = control(ControlRow::Sensitivity(InputDevice::KeyboardMouse));
        let buttons: Vec<PauseButton> = app.world_mut().query::<&PauseButton>().iter(app.world()).copied().collect();
        assert!(buttons.contains(&PauseButton::Meter(mouse)) && buttons.contains(&PauseButton::SliderStep(mouse, 1)), "a meter with arrows: {buttons:?}");
        let is_invert = |r: &RowId| *r == RowId::Control(ControlRow::Invert);
        choose_row(&mut app, ControlRow::Invert);
        assert_eq!(value_of(&mut app, is_invert).as_deref(), Some("OFF"));
        crate::bindings::set_invert_y(app.world_mut(), true);
        assert_eq!(value_of(&mut app, is_invert).as_deref(), Some("ON"));
    }

    #[derive(Resource)]
    struct ListSize(String);

    /// 1920x1080 out of reach while this is false.
    #[derive(Resource)]
    struct ListBig(bool);

    /// A settings page with one list row, its choices shown by words, 1920x1080 out of
    /// reach; the page's input and its drawing both run.
    fn reach_app() -> App {
        const NAMES: [&str; 3] = ["800x600", "1280x800", "1920x1080"];
        let mut app = App::new();
        app.insert_resource(ListSize("1280x800".into())).insert_resource(ListBig(false));
        app.init_resource::<Time>().init_resource::<PlayClock>().init_resource::<PauseAid>();
        app.init_resource::<ButtonInput<KeyCode>>().init_resource::<ButtonInput<MouseButton>>();
        app.insert_resource(NextState::<PlayState>::default()).init_resource::<PageRequest>().init_resource::<Listening>().init_resource::<SettingsLayout>().init_resource::<crate::graphics_resume::GraphicsResume>();
        app.init_resource::<LoadList>().init_resource::<Assets<Image>>().init_resource::<crate::save::SaveConfig>().init_resource::<PauseStatus>();
        app.init_resource::<crate::pad::PadGlyphs>().insert_resource(PauseLook::plain());
        app.add_message::<AppExit>().add_message::<crate::ui_sounds::UiSound>().add_message::<crate::save::SaveRequest>().add_message::<crate::save::LoadRequest>().add_message::<MainMenuRequest>();
        let row = Setting { key: "size", group: "Test", label: "Size", note: "How big.", switches: "", get: |_| false, set: |_, _| {}, slider: None };
        let list = crate::settings::ChoiceList {
            names: |_| NAMES.map(String::from).to_vec(),
            chosen: |w| w.resource::<ListSize>().0.clone(),
            pick: |w, n| {
                let ok = NAMES.contains(&n);
                if ok {
                    w.resource_mut::<ListSize>().0 = n.to_string();
                }
                ok
            },
            save: |w| w.resource::<ListSize>().0.clone(),
            load: |w, t| {
                let ok = NAMES.contains(&t);
                if ok {
                    w.resource_mut::<ListSize>().0 = t.to_string();
                }
                ok
            },
            ask_keep: false,
        };
        crate::settings::add_choice_list(&mut app, row, list);
        crate::settings::set_choice_labels(&mut app, "size", |_, n| ["Small", "Medium", "Large"][NAMES.iter().position(|x| *x == n).unwrap_or(0)].to_string());
        crate::settings::set_choice_unavailable(&mut app, "size", |w, n| (n == "1920x1080" && !w.resource::<ListBig>().0).then(|| "Needs a bigger monitor".to_string()));
        app.insert_resource(Paused(true)).insert_resource(Page::Settings { selected: Some(RowId::Setting(0)) }).init_resource::<PauseView>();
        app.add_systems(Update, (choose, draw).chain());
        app
    }

    fn texts(app: &mut App) -> Vec<(String, Color)> {
        let world = app.world_mut();
        world.query::<(&Text, &TextColor)>().iter(world).map(|(t, c)| (t.0.clone(), c.0)).collect()
    }

    fn note(app: &mut App) -> String {
        let world = app.world_mut();
        world.query_filtered::<&Text, With<SwitchNote>>().single(world).unwrap().0.clone()
    }

    fn row_state(app: &mut App) -> String {
        let world = app.world_mut();
        world.query::<(&SwitchState, &Text)>().single(world).unwrap().1.0.clone()
    }

    #[test]
    fn a_choice_out_of_reach_is_drawn_dim_gives_its_reason_and_is_not_taken_by_any_input() {
        let mut app = reach_app();
        app.update();
        // The box shows the chosen one's words; the state kept as text stays its name.
        assert_eq!(row_state(&mut app), "1280x800");
        assert!(texts(&mut app).contains(&("Medium".to_string(), color(Shade::Bright))), "{:?}", texts(&mut app));
        assert_eq!(note(&mut app), "How big.");
        // Enter opens the list on the chosen one; Down comes onto the one out of reach:
        // drawn dim, its reason under the rows.
        press_only(&mut app, KeyCode::Enter);
        app.update();
        assert_eq!(app.world().resource::<PauseView>().dropdown.map(|o| o.at), Some(1));
        press_only(&mut app, KeyCode::ArrowDown);
        app.update();
        assert_eq!(app.world().resource::<PauseView>().dropdown.map(|o| o.at), Some(2));
        let all = texts(&mut app);
        assert!(all.contains(&("Large".to_string(), color(Shade::Dim))) && all.contains(&("Small".to_string(), color(Shade::Bright))), "{all:?}");
        assert_eq!(note(&mut app), "Needs a bigger monitor");
        // Enter on it: nothing taken, the list stays open on it.
        press_only(&mut app, KeyCode::Enter);
        app.update();
        assert_eq!(app.world().resource::<ListSize>().0, "1280x800");
        assert_eq!(app.world().resource::<PauseView>().dropdown.map(|o| o.at), Some(2), "still open");
        // A click on it: the same.
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
        app.world_mut().spawn((Button, PauseButton::Choice(0, 2), PointerState::Pressed));
        app.update();
        assert_eq!(app.world().resource::<ListSize>().0, "1280x800");
        assert!(app.world().resource::<PauseView>().dropdown.is_some(), "still open");
        // Esc closes it; Right on the row passes over the one out of reach (nothing after
        // it), Left takes the one before.
        press_only(&mut app, KeyCode::Escape);
        app.update();
        assert!(app.world().resource::<PauseView>().dropdown.is_none());
        press_only(&mut app, KeyCode::ArrowRight);
        app.update();
        assert_eq!(app.world().resource::<ListSize>().0, "1280x800");
        press_only(&mut app, KeyCode::ArrowLeft);
        app.update();
        assert_eq!(app.world().resource::<ListSize>().0, "800x600");
        // Right from there passes over nothing: 1280x800; again: still not 1920x1080.
        press_only(&mut app, KeyCode::ArrowRight);
        app.update();
        assert_eq!(app.world().resource::<ListSize>().0, "1280x800");
        press_only(&mut app, KeyCode::ArrowRight);
        app.update();
        assert_eq!(app.world().resource::<ListSize>().0, "1280x800");
        // The settings aid asks for it by name: refused the same way.
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().reset_all();
        app.world_mut().resource_mut::<PauseAid>().switches = vec![(0.0, "size=1920x1080".into())];
        app.update();
        assert_eq!(app.world().resource::<ListSize>().0, "1280x800");
        // In reach again: drawn bright and taken as any other.
        app.world_mut().resource_mut::<ListBig>().0 = true;
        app.update();
        app.world_mut().resource_mut::<PauseAid>().switches = vec![(0.0, "size=1920x1080".into())];
        app.update();
        assert_eq!(app.world().resource::<ListSize>().0, "1920x1080");
        app.update();
        assert_eq!(row_state(&mut app), "1920x1080");
        assert!(texts(&mut app).contains(&("Large".to_string(), color(Shade::Bright))));
        // Out of reach once chosen (a stored request): the box shows it dim.
        app.world_mut().resource_mut::<ListBig>().0 = false;
        app.update();
        assert!(texts(&mut app).contains(&("Large".to_string(), color(Shade::Dim))));
        assert_eq!(row_state(&mut app), "1920x1080");
    }
}
