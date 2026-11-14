//! The renderer's switches on the pause menu's settings page (`nvf_play::settings`, drawn
//! by `nvf_play::pause`; `--play` only): the on/off choices of the command line that can
//! change while the game runs. Each reads and writes the state its module uses, so a
//! switch starts as the command line left it, then as `NVForever.ini` stored it (kept by
//! `nvf_play::settings` like every switch of the page), except that in a test run a
//! switch given on the command line keeps its state (`given_switches`, #1382).
//!
//! Left on the command line: what is fixed when the game starts (`--no-npc-packages`,
//! `--no-actors`, FaceGen, weapons, `--no-lodgen`), most numbers (`--radius`; the volumes
//! are the Sound tab, `nvf_play::audio_out::register_settings`)
//! and the test aids. The draw distances are sliders here, each a part of the
//! command line's distance (`draw_distance.rs`); the field of view, the frame-rate cap and the
//! anti-aliasing and shadow-filter choices are sliders whose steps are the choices.

use crate::draw_distance::{self, DrawClass, DrawDistances};
use crate::fly::FlyCam;
use crate::frame_pace::{self, DisplaySync, FramePace};
use crate::graphics::{self, Graphics};
use crate::grass::GrassConfig;
use crate::foot_ik::FootIkSettings;
use crate::head_look::HeadLookSettings;
use crate::lips::LipState;
use crate::lod::LodState;
use crate::nav::NavState;
use crate::perf_overlay::{FpsCounter, Low};
use crate::player_body::PlayerBody;
use crate::sky::SkyState;
use crate::upscaler::{DlssModel, UpscaleQuality, Upscaler, UpscalerChoice};
use crate::window_mode;
use crate::world::Streamer;
use crate::{Args, Overlay};
use bevy::prelude::*;
use nvf_play::pause::GROUP_MENUS;
use nvf_play::settings::{add_choice_list, add_setting, choice_index, choice_part, ChoiceList, Setting, Slider};

// The groups the rows register with, one short word each; which tab and heading the pause
// menu shows each row under is nvf_play::settings_layout's table, not the group (#1953).
const GROUP_WORLD: &str = "World";
const GROUP_PICTURE: &str = "Display";
const GROUP_DEVELOPER: &str = "Debug";
const GROUP_GRAPHICS: &str = "Graphics";

/// The settings switches this run's command line gave (`nvf_play::settings::GivenSwitches`):
/// the `--name` of each argument, without the `--size` a run taking over a load is given
/// (the window's size then, handoff.rs). They win over the INI in a test run: a timed,
/// measured or pictured run, whose results must be those of its switches (#1382).
pub(crate) fn given_switches(args: &Args, line: &[String]) -> nvf_play::settings::GivenSwitches {
    let handoff = line.iter().any(|a| a == "--handoff" || a.starts_with("--handoff="));
    let given = line
        .iter()
        .filter(|a| a.starts_with("--"))
        .map(|a| a.split('=').next().unwrap_or(a).to_string())
        .filter(|a| !(handoff && a == "--size"))
        .collect();
    let win = args.run_seconds.is_some() || args.log_frame_times.is_some() || args.screenshot.is_some() || args.timelapse.is_some() || args.frame_json.is_some();
    nvf_play::settings::GivenSwitches { given, win }
}

/// List the renderer's switches: those whose module is part of this run.
pub fn register(app: &mut App) {
    let given = given_switches(app.world().resource::<Args>(), &crate::command_line());
    app.insert_resource(given);
    let has_lips = app.world().contains_resource::<LipState>();
    let has_heads = app.world().contains_resource::<HeadLookSettings>();
    let has_feet = app.world().contains_resource::<FootIkSettings>();
    let has_nav = app.world().contains_resource::<NavState>();
    let has_body = app.world().contains_resource::<PlayerBody>();
    if has_lips {
        add_setting(
            app,
            Setting {
                key: "lip-sync",
                group: GROUP_MENUS,
                label: "Lip sync",
                note: "On: a speaker's face moves with the line. Off: a face that is moving eases back to rest. (--no-lip)",
                get: |w| w.resource::<LipState>().enabled,
                set: |w, on| w.resource_mut::<LipState>().enabled = on,
                slider: None,
            },
        );
    }
    if has_heads {
        add_setting(
            app,
            Setting {
                key: "head-tracking",
                group: GROUP_WORLD,
                label: "Head tracking",
                note: "On: actors turn their heads to the player and to others near them. Off: heads ease back to their animation. (--no-head-tracking)",
                get: |w| w.resource::<HeadLookSettings>().enabled,
                set: |w, on| w.resource_mut::<HeadLookSettings>().enabled = on,
                slider: None,
            },
        );
    }
    if has_feet {
        add_setting(
            app,
            Setting {
                key: "foot-ik",
                group: GROUP_WORLD,
                label: "Feet follow the ground",
                note: "On: on slopes and steps actors bend their knees and set each foot on the ground under it. Off: the legs play the animation as it is. (--no-foot-ik)",
                get: |w| w.resource::<FootIkSettings>().enabled,
                set: |w, on| w.resource_mut::<FootIkSettings>().enabled = on,
                slider: None,
            },
        );
    }
    add_setting(
        app,
        Setting {
            key: "sky-clock",
            group: GROUP_WORLD,
            label: "Sky follows the game clock",
            note: "On: the sky shows the game's time of day as it passes. Off: the sky keeps its hour while the game's clock runs on ([ and ] still set it). (--still-sky)",
            get: |w| !w.resource::<Args>().still_sky,
            set: |w, on| w.resource_mut::<Args>().still_sky = !on,
            slider: None,
        },
    );
    // The resolution first, then full screen [owner 2026-10-18].
    add_choice_list(
        app,
        Setting {
            key: "resolution",
            group: GROUP_PICTURE,
            label: "Resolution",
            note: "The window's size, or in full screen the monitor's (its own size: borderless). A new one goes back after 20 seconds unless you keep it. (--size)",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        ChoiceList {
            names: window_mode::resolution_names,
            chosen: window_mode::resolution_chosen,
            pick: window_mode::resolution_pick,
            save: window_mode::resolution_save,
            load: window_mode::resolution_load,
            ask_keep: true,
        },
    );
    add_setting(
        app,
        Setting {
            key: "borderless",
            group: GROUP_PICTURE,
            label: "Full screen",
            note: "On: the game fills the monitor it is on, without a frame, at the resolution chosen above for full screen. Off: a window of the resolution chosen for a window. (--window borderless)",
            get: |w| w.resource::<Args>().window == window_mode::WindowChoice::Borderless,
            set: window_mode::set_borderless,
            slider: None,
        },
    );
    add_choice_list(
        app,
        Setting {
            key: "monitor",
            group: GROUP_PICTURE,
            label: "Monitor",
            note: "The display the game opens on in full screen (the first display until you pick one). (--window borderless)",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        ChoiceList {
            names: window_mode::monitor_names,
            chosen: window_mode::monitor_chosen,
            pick: window_mode::monitor_pick,
            save: window_mode::monitor_save,
            load: window_mode::monitor_load,
            ask_keep: false,
        },
    );
    if has_body {
        add_setting(
            app,
            Setting {
                key: "first-person-body",
                group: GROUP_PICTURE,
                label: "Body in first person",
                note: "On: in first person you see your own body and legs when you look down. Off: only the arms and the weapon are drawn, as in the original. (--first-person-body)",
                get: |w| w.resource::<PlayerBody>().first_person_body,
                set: |w, on| w.resource_mut::<PlayerBody>().first_person_body = on,
                slider: None,
            },
        );
        add_setting(
            app,
            Setting {
                key: "weapon-sway",
                group: GROUP_PICTURE,
                label: "Weapon sway",
                note: "On: in first person the arms and weapon lag behind a turn and bob as you walk and run. Off: they stay still in the view. (--no-weapon-sway)",
                get: |w| w.resource::<PlayerBody>().weapon_sway,
                set: |w, on| w.resource_mut::<PlayerBody>().weapon_sway = on,
                slider: None,
            },
        );
        add_setting(
            app,
            Setting {
                key: "head-bob",
                group: GROUP_PICTURE,
                label: "Head bob",
                note: "On: in first person the view dips with each step as you walk and run. Off: it stays level. (--no-head-bob)",
                get: |w| w.resource::<PlayerBody>().head_bob,
                set: |w, on| w.resource_mut::<PlayerBody>().head_bob = on,
                slider: None,
            },
        );
        add_setting(
            app,
            Setting {
                key: "weapon-lower",
                group: GROUP_PICTURE,
                label: "Lower weapon near walls",
                note: "On: in first person a drawn weapon is lowered when a wall or a friendly person is right in front of you, and comes straight back up when you aim or fire. Off: it stays up and can poke through them, as in the original. Lower Weapon (H) lowers or raises it by hand either way. (--no-weapon-lower)",
                get: |w| w.resource::<PlayerBody>().weapon_lower,
                set: |w, on| w.resource_mut::<PlayerBody>().weapon_lower = on,
                slider: None,
            },
        );
        add_setting(
            app,
            Setting {
                key: "true-iron-sights",
                group: GROUP_MENUS,
                label: "True iron sights",
                note: "On: aiming a gun lines its sights up with your eye. The crosshair is hidden while you aim down them. Off: the gun is held low with the crosshair, as in the original. Guns with no sights to look down (laser rifle, minigun) aim as the original either way. (--no-true-iron-sights)",
                get: |w| w.resource::<PlayerBody>().true_iron_sights,
                set: |w, on| w.resource_mut::<PlayerBody>().true_iron_sights = on,
                slider: None,
            },
        );
        add_setting(
            app,
            Setting {
                key: "scope-3d",
                group: GROUP_MENUS,
                label: "3D scopes (test)",
                note: "On: aiming a scoped gun keeps the gun on screen and shows the zoomed view in its scope's lens, with the reticle; the view around it zooms only a little. Off: the full-screen scope picture with its black border, as in the original. Guns whose scope lens is not known yet keep the picture. (--scope-3d)",
                get: |w| w.resource::<nvf_play::aim_zoom::Scope3d>().on,
                set: |w, on| w.resource_mut::<nvf_play::aim_zoom::Scope3d>().on = on,
                slider: None,
            },
        );
    }
    {
        use crate::shadow_rate::{set_sun_shadow_choice, sun_shadow_choice, SUN_SHADOW_CHOICES};
        let n = SUN_SHADOW_CHOICES.len();
        add_setting(
            app,
            Setting::slider(
                "sun-shadows",
                GROUP_PICTURE,
                "Sun shadows",
                "The sun's shadows, out to --shadow-distance: Off; Performance (the shadow maps drawn every other frame in two cascades, trees casting shadows only near you, the first-person arms taking the world's shadows: saves frame time outdoors, shadows of moving things update at half the frame rate); or Quality (three cascades, the farther ones drawn less often). The original casts none from the world. Quality by default in the game. (--sun-shadows, --sun-shadows=performance, --no-sun-shadows)",
                Slider::choice(n, |w| choice_part(sun_shadow_choice(w), SUN_SHADOW_CHOICES.len()), |w, v| set_sun_shadow_choice(w, choice_index(v, SUN_SHADOW_CHOICES.len())), |v| {
                    SUN_SHADOW_CHOICES[choice_index(v, SUN_SHADOW_CHOICES.len())].to_string()
                }),
            ),
        );
    }
    add_setting(
        app,
        Setting {
            key: "grass",
            group: GROUP_PICTURE,
            label: "Grass",
            note: "On: grass grows on the landscape; the cells around you get theirs over the next frames. Off: it is hidden and none grows on new cells. (--no-grass)",
            get: |w| w.resource::<GrassConfig>().enabled,
            set: |w, on| w.resource_mut::<GrassConfig>().enabled = on,
            slider: None,
        },
    );
    add_setting(
        app,
        Setting {
            key: "distant-lod",
            group: GROUP_PICTURE,
            label: "Distant terrain and objects",
            note: "On: distant land, buildings and trees are drawn beyond the loaded cells (outdoors). Off: the world ends at the loaded cells. (--no-lod)",
            get: |w| !w.resource::<Args>().no_lod,
            set: set_lod,
            slider: None,
        },
    );
    add_setting(
        app,
        Setting {
            key: "grade",
            group: GROUP_PICTURE,
            label: "Colour grade",
            note: "On: the picture is graded by the worldspace's, the weather's and the cell's image space. Off: the picture as lit, for comparisons. (--no-grade)",
            get: |w| w.resource::<SkyState>().grade,
            set: |w, on| w.resource_mut::<SkyState>().grade = on,
            slider: None,
        },
    );
    register_display(app);
    register_distances(app);
    register_graphics(app);
    // The Quality and Enhanced effects rows over the rows above (#1966).
    crate::quality::register(app, GROUP_GRAPHICS);
    // Renderer edits are latched by the shared settings setters, including Defaults.
    // Draw-distance streaming has no resume readiness contract yet (#1820) [design].
    register_pipeline_waits(app);
    // The volumes after the picture's rows (shown on the Audio tab: nvf_play::settings_layout).
    nvf_play::audio_out::register_settings(app);
    add_setting(
        app,
        Setting {
            key: "overlay",
            group: GROUP_DEVELOPER,
            label: "Text overlay",
            note: "On: the position, loading counts and key help are written over the picture. Off by default in the game. (--overlay, --no-overlay)",
            get: |w| !w.resource::<Args>().no_overlay,
            set: set_overlay,
            slider: None,
        },
    );
    add_setting(
        app,
        Setting {
            key: "hit-markers",
            group: GROUP_DEVELOPER,
            label: "Shot markers",
            note: "On: a cross is drawn at each impact and a line from the muzzle along each hitscan shot. (--hit-markers)",
            get: |w| w.resource::<nvf_play::combat::HitMarks>().show,
            set: |w, on| w.resource_mut::<nvf_play::combat::HitMarks>().show = on,
            slider: None,
        },
    );
    add_setting(
        app,
        Setting {
            key: "god-mode",
            group: GROUP_DEVELOPER,
            label: "God mode",
            note: "On: the player takes no damage from hits and cannot be killed by them. (--god)",
            get: |_| nvf_play::combat::god_mode(),
            set: |_, on| nvf_play::combat::set_god_mode(on),
            slider: None,
        },
    );
    if has_nav {
        add_setting(
            app,
            Setting {
                key: "navmesh",
                group: GROUP_DEVELOPER,
                label: "Draw the navmesh",
                note: "On: the navmesh near the camera and the walking NPCs' paths are drawn. (--show-navmesh)",
                get: |w| w.resource::<NavState>().show,
                set: |w, on| w.resource_mut::<NavState>().show = on,
                slider: None,
            },
        );
    }
}

/// The Frame rate counter switch's note; the switch it names (`--fps`) decides with the
/// INI whether the frame split takes GPU timestamps from the start
/// (`perf_overlay::split::timestamps_wanted`), as `nvf_play::settings::apply_stored` reads it.
pub(crate) const FPS_COUNTER_NOTE: &str = "On: a small panel of performance readings at the top right of the screen, the Counter rows below choosing what it shows; with only its frame rate row on it is a frame rate counter. Not in the original. (--fps)";

/// The upscaling row's note (#2242), NVForever's own words [design].
pub(crate) const FPS_UPSCALING_NOTE: &str = "The upscaler that draws now: its name and mode, the size the world is drawn at and the size it is rebuilt to (with FSR 3.1 and Bilinear from its mode, with DLSS the size DLSS chose), and with DLSS the model asked for. Reads Off and the window's size while nothing upscales, also when the upscaler asked for cannot run on this computer. Follows a change within a frame or two, showing the upscaler without sizes until then.";

/// The frame split row's note (no command-line switch: the INI's state always applies).
pub(crate) const FPS_SPLIT_NOTE: &str = "What a frame took on the main thread (the game's own work), on the render thread (preparing the picture, including the wait for the window's next image, shown in brackets: with vertical sync that wait is the display's time, not work) and on the graphics card (the render passes it times, from GPU timestamps), each the mean over half a second. A frame held up by the processor has a long main time, or a long render time beside a short wait; the GPU time is what tells a frame held up by the graphics card. The GPU part needs this row and the counter on when the game starts (it takes effect at the next start), and a graphics card that offers timestamps; without them it is left out. Unless a measuring aid also uses them, the timestamps are taken only while the row shows.";

/// The field of view's least and greatest degrees on the settings page, in steps of 5.
const FOV_MIN: f32 = 50.0;
const FOV_MAX: f32 = 110.0;
const FOV_CHOICES: usize = 13;

/// The field of view (`--fov`), the frame rate counter (the performance overlay,
/// `perf_overlay/`: its switch and a switch per row) and how fast frames are
/// shown (`frame_pace.rs`; not while `--log-frame-times` measures, which wants vsync off).
fn register_display(app: &mut App) {
    add_setting(
        app,
        Setting::slider(
            "field-of-view",
            GROUP_PICTURE,
            "Field of view",
            "The game's field of view in degrees across a 4:3 picture (75 as the original's default); wider screens see more at the sides. (--fov)",
            Slider::choice(FOV_CHOICES, |w| fov_part(w.resource::<Args>().fov), set_fov, |v| format!("{:.0}", fov_degrees(v))),
        ),
    );
    // The HUD's size (#1631), kept by nvf-play's interface and fixed for the run at start.
    add_setting(app, nvf_play::interface::ui_scale_row(GROUP_PICTURE));
    add_setting(
        app,
        Setting {
            key: "fps-counter",
            group: GROUP_PICTURE,
            label: "Frame rate counter",
            note: FPS_COUNTER_NOTE,
            get: |w| w.resource::<FpsCounter>().show,
            set: |w, on| w.resource_mut::<FpsCounter>().show = on,
            slider: None,
        },
    );
    register_counter_rows(app);
    if app.world().resource::<Args>().log_frame_times.is_some() {
        return;
    }
    add_choice_list(
        app,
        Setting {
            key: "display-sync",
            group: GROUP_PICTURE,
            label: "Display sync",
            note: "Vertical sync: each frame waits for the display's refresh (no tearing). Variable refresh: for a G-Sync or FreeSync screen, vertical sync with the frame rate held a few frames under the screen's refresh rate, so frames are shown as soon as they are drawn without tearing. Off: frames are shown as soon as they are drawn (may tear).",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        ChoiceList {
            names: |_| DisplaySync::ALL.iter().map(|d| d.name().to_string()).collect(),
            chosen: |w| w.resource::<FramePace>().sync.name().to_string(),
            pick: set_display_sync,
            save: |w| w.resource::<FramePace>().sync.stored().to_string(),
            load: set_display_sync,
            ask_keep: false,
        },
    );
    app.add_systems(PostStartup, read_old_vsync.after(nvf_play::settings::apply_stored));
    let caps = frame_pace::CAPS.len();
    add_setting(
        app,
        Setting::slider(
            "frame-cap",
            GROUP_PICTURE,
            "Frame rate limit",
            "At most this many frames a second (Off: as many as the computer draws, or the display's refresh with vertical sync; with variable refresh the lower of this and its own limit). A lower limit keeps the computer cooler and quieter.",
            Slider::choice(
                caps,
                |w| {
                    let cap = w.resource::<FramePace>().cap;
                    choice_part(frame_pace::CAPS.iter().position(|&c| c == cap).unwrap_or(0), frame_pace::CAPS.len())
                },
                |w, v| w.resource_mut::<FramePace>().cap = frame_pace::CAPS[choice_index(v, frame_pace::CAPS.len())],
                |v| match frame_pace::CAPS[choice_index(v, frame_pace::CAPS.len())] {
                    0 => "Off".to_string(),
                    c => c.to_string(),
                },
            ),
        ),
    );
}

/// The frame rate counter's rows, listed under its switch (issue #1868; the
/// defaults are the issue's, `perf_overlay::Rows`). Each note says what the row reads and
/// where from.
fn register_counter_rows(app: &mut App) {
    let row = |key: &'static str, label: &'static str, note: &'static str, get: fn(&World) -> bool, set: fn(&mut World, bool)| Setting { key, group: GROUP_PICTURE, label, note, get, set, slider: None };
    add_setting(
        app,
        row(
            "fps-counter-fps",
            "Counter: frame rate",
            "The frames a second and the mean frame time of the last half second, and the 1% low (the mean of the slowest hundredth of the frames of the last 10 seconds) or the slowest frame of the last 10 seconds, as the row below chooses. Measured from the time between frames. While frame generation shows generated frames, the frames shown a second come first and the rendered ones in brackets; the frame times are the rendered frames'.",
            |w| w.resource::<FpsCounter>().rows.fps,
            |w, on| w.resource_mut::<FpsCounter>().rows.fps = on,
        ),
    );
    add_choice_list(
        app,
        Setting {
            key: "fps-counter-low",
            group: GROUP_PICTURE,
            label: "Counter: last figure",
            note: "The frame rate row's last figure, over the last 10 seconds: 1% low, the mean of the slowest hundredth of the frames; Slowest frame, the single slowest.",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        ChoiceList {
            names: |_| Low::ALL.iter().map(|l| l.name().to_string()).collect(),
            chosen: |w| w.resource::<FpsCounter>().rows.low.name().to_string(),
            pick: set_counter_low,
            save: |w| w.resource::<FpsCounter>().rows.low.stored().to_string(),
            load: set_counter_low,
            ask_keep: false,
        },
    );
    let rows = [
        row(
            "fps-counter-graph",
            "Counter: frame graph",
            "The frame times of the last 5 seconds as a line under the frame rate, the newest at the right, with a faint line at a 60 Hz frame (16.7 ms); its top is the slowest frame drawn, or two 60 Hz frames when every frame was faster, so a spike reaches the top. Measured from the time between frames, drawn again twice a second.",
            |w| w.resource::<FpsCounter>().rows.graph,
            |w, on| w.resource_mut::<FpsCounter>().rows.graph = on,
        ),
        row(
            "fps-counter-split",
            "Counter: frame split",
            FPS_SPLIT_NOTE,
            |w| w.resource::<FpsCounter>().rows.split,
            |w, on| w.resource_mut::<FpsCounter>().rows.split = on,
        ),
        // [design, #2249] Availability and scope follow perf_overlay/usage.rs.
        row(
            "fps-counter-cpu",
            "Counter: CPU use",
            "Machine and game CPU use, read once a second while shown. Windows uses performance counters; Linux uses /proc. Missing machine readings say Not available. Game use is cores kept busy: 1.0 is one full core.",
            |w| w.resource::<FpsCounter>().rows.cpu,
            |w, on| w.resource_mut::<FpsCounter>().rows.cpu = on,
        ),
        row(
            "fps-counter-gpu",
            "Counter: GPU use",
            "GPU use, read once a second while shown. Windows: busiest engine across programs. Linux: NVIDIA NVML, then amdgpu, then DRM client time. DRM reads game use and says game; the other Linux sources read whole-device use. Missing readings say Not available.",
            |w| w.resource::<FpsCounter>().rows.gpu,
            |w, on| w.resource_mut::<FpsCounter>().rows.gpu = on,
        ),
        row(
            "fps-counter-vram",
            "Counter: GPU memory",
            "The graphics memory the game holds against the budget the driver gives it (Vulkan's memory budget, read every 2 seconds). It is the driver's figure: it counts the blocks kept empty for reuse, so it is what the game holds, not the size of what is drawn. Left out on a graphics chip that shares the machine's RAM, which has no budget of its own. Marked ! while the game keeps fewer unused models and textures because it is near the budget.",
            |w| w.resource::<FpsCounter>().rows.vram,
            |w, on| w.resource_mut::<FpsCounter>().rows.vram = on,
        ),
        row(
            "fps-counter-ram",
            "Counter: RAM",
            "The game's memory in RAM (its working set, from Windows once a second while the counter shows) and the share of the machine's RAM in use (read every 2 seconds). Marked ! while the game keeps fewer unused models and textures because the machine has under a tenth of its RAM free. Windows only.",
            |w| w.resource::<FpsCounter>().rows.ram,
            |w, on| w.resource_mut::<FpsCounter>().rows.ram = on,
        ),
        row(
            "fps-counter-loaded",
            "Counter: loaded",
            "What the game holds now: the cells loaded in the physics world, the placed objects with a model, the actors with a skeleton and every entity of the engine, counted twice a second while the row shows (the stress tests' counts).",
            |w| w.resource::<FpsCounter>().rows.loaded,
            |w, on| w.resource_mut::<FpsCounter>().rows.loaded = on,
        ),
        // [design, #2242] What draws is the effective choice (`upscaler::effective_temporal`),
        // its sizes what the render world drew (`upscaler::UpscaleDrawn`).
        row(
            "fps-counter-upscaling",
            "Counter: upscaling",
            FPS_UPSCALING_NOTE,
            |w| w.resource::<FpsCounter>().rows.upscale,
            |w, on| w.resource_mut::<FpsCounter>().rows.upscale = on,
        ),
    ];
    for r in rows {
        add_setting(app, r);
    }
}

/// The frame rate row's last figure of this name (or INI text, `Low::parse`); false: not
/// one.
fn set_counter_low(world: &mut World, name: &str) -> bool {
    let Some(low) = Low::parse(name) else { return false };
    world.resource_mut::<FpsCounter>().rows.low = low;
    true
}

/// `[Settings] <key>` of the INI this run reads (`SettingsFile::for_run`), for what has to
/// be known when the app is built, before the INI is applied (`apply_stored`, PostStartup):
/// the CPU placement (`cpu_placement.rs`), the frame split's GPU timestamps
/// (`perf_overlay::split`). None without the INI or the key.
pub(crate) fn stored_at_start(key: &str) -> Option<String> {
    fnv_install::ini_get(&settings_text_at_start()?, nvf_play::settings::SECTION, key)
}

/// The text of the INI this run reads (`SettingsFile::for_run`), as [`stored_at_start`] reads
/// it: for the upscaler's rows, read before the renderer exists (`UpscalerIni`). None without
/// the INI.
pub(crate) fn settings_text_at_start() -> Option<String> {
    // Unattended: a test aid drives this run (as nvf_play's test_aid::Unattended).
    let unattended = std::env::vars_os().any(|(k, _)| k.to_str().is_some_and(nvf_play::drives_run));
    let path = nvf_play::settings::SettingsFile::for_run(unattended).0?;
    std::fs::read_to_string(path).ok()
}

/// The display sync of this name (or INI text, `DisplaySync::parse`); false: not one.
fn set_display_sync(world: &mut World, name: &str) -> bool {
    let Some(d) = DisplaySync::parse(name) else { return false };
    world.resource_mut::<FramePace>().sync = d;
    true
}

/// An INI written before the display-sync row had the `vsync` switch: read it when there is
/// no `display-sync` (after `nvf_play::settings::apply_stored`, so it counts as stored).
fn read_old_vsync(world: &mut World) {
    if world.resource::<Args>().log_frame_times.is_some() {
        return;
    }
    let Some(path) = world.get_resource::<nvf_play::settings::SettingsFile>().and_then(|f| f.0.clone()) else { return };
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let section = nvf_play::settings::SECTION;
    if fnv_install::ini_get(&text, section, "display-sync").is_some() {
        return;
    }
    let Some(old) = fnv_install::ini_get(&text, section, "vsync") else { return };
    match DisplaySync::parse(&old) {
        Some(d) => {
            world.resource_mut::<FramePace>().sync = d;
            info!("settings: display-sync {} from the old vsync={} in {}", d.name(), old.trim(), path.display());
        }
        None => warn!("settings: [{section}] vsync={old} in {} is not 1 or 0; left as it is", path.display()),
    }
}

/// The field of view's degrees for a slider part, whole.
fn fov_degrees(v: f32) -> f32 {
    (FOV_MIN + v.clamp(0.0, 1.0) * (FOV_MAX - FOV_MIN)).round()
}

/// The slider part for `deg` degrees, as `Slider::fit` keeps parts.
fn fov_part(deg: f32) -> f32 {
    ((deg - FOV_MIN) / (FOV_MAX - FOV_MIN) * 1000.0).round().clamp(0.0, 1000.0) / 1000.0
}

/// A new field of view: kept in `Args` and put on the camera.
fn set_fov(world: &mut World, v: f32) {
    let deg = fov_degrees(v);
    world.resource_mut::<Args>().fov = deg;
    let mut cams = world.query_filtered::<&mut Projection, With<FlyCam>>();
    for mut p in cams.iter_mut(world) {
        if let Projection::Perspective(p) = &mut *p {
            p.fov = nvf_play::free_cam::vertical_fov(deg);
        }
    }
}

/// The draw-distance sliders (`draw_distance.rs`): each a part of its full distance, the
/// full one drawing as without them.
fn register_distances(app: &mut App) {
    if !app.world().contains_resource::<DrawDistances>() {
        return;
    }
    let slider = |get: fn(&World) -> f32, set: fn(&mut World, f32)| Slider { get, set, min: draw_distance::MIN, step: draw_distance::STEP, shown: None };
    let sliders = [
        Setting::slider(
            "object-distance",
            GROUP_PICTURE,
            "Object distance",
            "How far from you objects (buildings, rocks, furniture, clutter) of the loaded cells are drawn, as a part of the farthest those cells reach. Lower draws fewer and runs faster.",
            slider(|w| w.resource::<DrawDistances>().objects, |w, v| draw_distance::set_part(w, DrawClass::Object, v)),
        ),
        Setting::slider(
            "actor-distance",
            GROUP_PICTURE,
            "Actor distance",
            "How far from you people and creatures are drawn, as a part of the farthest the loaded cells reach.",
            slider(|w| w.resource::<DrawDistances>().actors, |w, v| draw_distance::set_part(w, DrawClass::Actor, v)),
        ),
        Setting::slider(
            "item-distance",
            GROUP_PICTURE,
            "Item distance",
            "How far from you items lying in the world (weapons, ammo, aid, notes, caps, clutter you can take) are drawn, as a part of the farthest the loaded cells reach.",
            slider(|w| w.resource::<DrawDistances>().items, |w, v| draw_distance::set_part(w, DrawClass::Item, v)),
        ),
        Setting::slider(
            "shadow-distance",
            GROUP_PICTURE,
            "Shadow distance",
            "How far from you the sun casts shadows (with Sun shadows on), as a part of --shadow-distance.",
            slider(|w| w.resource::<DrawDistances>().shadows, draw_distance::set_shadows),
        ),
        Setting::slider(
            "grass-distance",
            GROUP_PICTURE,
            "Grass distance",
            "How far from you grass is drawn, as a part of --grass-distance and its fade range.",
            slider(|w| w.resource::<DrawDistances>().grass, draw_distance::set_grass),
        ),
    ];
    for s in sliders {
        add_setting(app, s);
    }
}

/// The Upscaler row's key and the Upscaling mode row's: two rows for the one request
/// (`Graphics::upscaler`) since #2198's play-test, where the one dropdown of eleven choices
/// was too long for its box [design: the user's, 2026-11-12]. The second row was labelled
/// Upscaling quality at first; renamed to keep it apart from the Quality preset row (the
/// user's, 2026-11-12), its key kept.
pub(crate) const UPSCALER_KEY: &str = "upscaler";
pub(crate) const QUALITY_KEY: &str = "upscaler-quality";

/// The one row the two took over from, under the name it had when FSR 3.1 was its only
/// upscaler: still read from an INI that lacks `upscaler` (`set_older_key`) and still taken by
/// the test aids (`set_list_alias`), by its eleven names (`graphics::FSR_UPSCALING_CHOICES`)
/// and the early slider build's numbers.
pub(crate) const LEGACY_KEY: &str = "fsr-upscaling";

/// The two rows' notes. Each names all three command-line switches, so a test run that gives
/// any of them keeps both rows at the command line's choice over the INI
/// (`nvf_play::settings::GivenSwitches`), at the start ([`upscaler_after_ini`]) as at
/// `PostStartup`.
pub(crate) const UPSCALER_NOTE: &str = "Reconstruct the world at display resolution with FSR 3.1 or NVIDIA DLSS, or only stretch it there with Bilinear (softer; the cheapest, on any graphics card), at the Upscaling mode below. Disables MSAA and TAA while active; FSR 3.1 and DLSS also disable FXAA and SMAA. Menus stay sharp. FSR 3.1 and DLSS require Vulkan; unsupported devices keep native rendering. DLSS needs an NVIDIA RTX graphics card and NVIDIA's DLSS files in the dlss folder beside nvforever.exe (NVForever ships none). With the files there when the game starts, every choice takes effect at once; files put there later are found at the next start, so DLSS chosen until then reads DLSS (restart) (loading a save starts the game again). Sharpening controls FSR sharpening. Frame generation needs FSR 3.1 or DLSS. (--upscaler, --upscaler-quality, --fsr)";
pub(crate) const QUALITY_NOTE: &str = "The share of the screen's width and height the world is drawn at; the upscaler rebuilds the rest. Native AA: full resolution, anti-aliasing only (DLSS: DLAA; Bilinear: no change). Quality: the sharpest upscaled look, for 1440p and 4K. Balanced: more speed with little visible loss. Performance: the highest frame rate, for 4K or heavy scenes. Ultra Performance: for very high resolutions only. Takes effect at once. With Upscaler Off there is nothing to set. (--upscaler-quality, --upscaler, --fsr)";

/// The DLSS model row's key, right under Upscaling mode (#2198, the user's request): which
/// of NVIDIA's DLSS models DLSS uses (`Graphics::dlss_model`).
pub(crate) const MODEL_KEY: &str = "dlss-model";

/// The DLSS model row's note: each choice in NVForever's own words, in the row's order, from
/// what Streamline's header says of the presets [documented: `sl_dlss.h:46-65`] and the
/// defaults NGX reported in the runs at 1280x720 [observed, 2026-11-12:
/// docs/spec-notes/streamline.md, "DLSS presets"]. The settings page has no note per choice,
/// so the row's note holds them all; it says the letters are what NVIDIA calls presets.
pub(crate) const MODEL_NOTE: &str = "Which of NVIDIA's DLSS models rebuilds the picture (NVIDIA calls them presets). Default (recommended): NVIDIA picks per mode: K for Native AA, Quality and Balanced, M for Performance, L for Ultra Performance. K: transformer model, best detail, costs more. L: sharper and steadier, less ghosting, costs the most. M: close to L in looks, closer to K in speed. J: like K with a little less ghosting and more flicker. Takes effect at once. Only while DLSS draws. (--dlss-model)";

/// The engine-level improvements of `graphics.rs` (`--graphics enhanced` starts with all
/// of them on, `vanilla` with none).
fn register_graphics(app: &mut App) {
    type Switch = (&'static str, &'static str, &'static str, fn(&World) -> bool, fn(&mut World, bool));
    // Two rows for the one upscaler request (#2198): which upscaler (Bilinear since #2414),
    // then its quality; both keep `Graphics::upscaler`. The quality row stands dimmed while
    // no upscaler is chosen. The older single row's INI key and test-aid key still work.
    add_choice_list(
        app,
        Setting { key: UPSCALER_KEY, group: GROUP_GRAPHICS, label: "Upscaler", note: UPSCALER_NOTE, get: |_| false, set: |_, _| {}, slider: None },
        ChoiceList {
            names: upscaler_names,
            chosen: upscaler_chosen,
            pick: pick_upscaler,
            save: upscaler_chosen,
            load: load_upscaler,
            ask_keep: false,
        },
    );
    nvf_play::settings::set_choice_labels(app, UPSCALER_KEY, upscaler_label);
    nvf_play::settings::set_choice_unavailable(app, UPSCALER_KEY, upscaler_unavailable);
    // Download DLSS… opens its dialog: nothing of the row to keep or store (#2250).
    nvf_play::settings::set_choice_action(app, UPSCALER_KEY, crate::dlss_download::is_choice);
    nvf_play::settings::set_older_key(app, UPSCALER_KEY, LEGACY_KEY);
    nvf_play::settings::set_older_split(app, LEGACY_KEY, split_legacy);
    add_choice_list(
        app,
        Setting { key: QUALITY_KEY, group: GROUP_GRAPHICS, label: "Upscaling mode", note: QUALITY_NOTE, get: |_| false, set: |_, _| {}, slider: None },
        ChoiceList {
            names: |_| UpscaleQuality::ALL.iter().map(|q| q.name().to_string()).collect(),
            chosen: quality_chosen,
            pick: load_quality,
            save: quality_chosen,
            load: load_quality,
            ask_keep: false,
        },
    );
    nvf_play::settings::set_choice_labels(app, QUALITY_KEY, quality_label);
    nvf_play::settings::set_disabled_status(app, QUALITY_KEY, |w| w.resource::<Graphics>().upscaler.quality().is_none().then_some("Needs an upscaler"));
    // The DLSS model, under the mode: dimmed while DLSS is not what draws.
    add_choice_list(
        app,
        Setting { key: MODEL_KEY, group: GROUP_GRAPHICS, label: "DLSS model", note: MODEL_NOTE, get: |_| false, set: |_, _| {}, slider: None },
        ChoiceList {
            names: |_| DlssModel::ALL.iter().map(|m| m.name().to_string()).collect(),
            chosen: model_chosen,
            pick: pick_model,
            save: model_chosen,
            load: load_model,
            ask_keep: false,
        },
    );
    nvf_play::settings::set_choice_labels(app, MODEL_KEY, |_, name| fnv_util::words::tr_str(name).to_string());
    nvf_play::settings::set_disabled_status(app, MODEL_KEY, model_dimmed);
    nvf_play::settings::set_list_alias(
        app,
        LEGACY_KEY,
        nvf_play::settings::ListAlias { rows: &[UPSCALER_KEY, QUALITY_KEY], chosen: legacy_chosen, pick: pick_legacy, unavailable: legacy_unavailable },
    );
    // The FSR library, under the DLSS model: dimmed while FSR is not what draws.
    add_choice_list(
        app,
        Setting {
            key: "fsr-library",
            group: GROUP_GRAPHICS,
            label: "FSR library",
            note: "What FSR upscales with: NVForever's built-in FSR 3.1, or a FidelityFX library in the game's folder (amd_fidelityfx_loader_vk.dll or amd_fidelityfx_vk.dll; not part of NVForever), which tools that replace the upscaler use. The choice names the library found; with none there the built-in FSR runs.",
            get: |_| false,
            set: |_, _| {},
            slider: None,
        },
        ChoiceList {
            names: |_| fsr_library_names().to_vec(),
            chosen: |_| fsr_library_names()[usize::from(crate::ffx_api::enabled())].clone(),
            pick: |_, name| pick_fsr_library(name),
            save: |_| (if crate::ffx_api::enabled() { FSR_LIBRARY_FOLDER } else { FSR_LIBRARY_BUILT_IN }).to_string(),
            load: |_, value| pick_fsr_library(value),
            ask_keep: false,
        },
    );
    nvf_play::settings::set_disabled_status(app, "fsr-library", fsr_library_dimmed);
    let choices = [
        Setting::slider(
            "anti-aliasing",
            GROUP_GRAPHICS,
            "Anti-aliasing",
            "How edges are smoothed: multisampling Off, 2x, 4x or 8x (--msaa; the cut-out edges of plants too), or the picture smoothed afterwards: FXAA (cheapest, softest), SMAA (sharper, a little dearer) or TAA (over the frames; plants too, soft while moving; --graphics-only fxaa, smaa or taa). Multisampling is the dearest, more at each step: at 1920x1080 on an RTX 4070, 2x added about 0.8 ms a frame, 4x about 1.3 ms and 8x about 2.3 ms (Goodsprings and Novac at noon); FXAA, SMAA and TAA cost less than 4x. Where the processor sets the pace (the Strip at night) the choice costs next to nothing. While FSR 3.1 or DLSS is on this row reads Off (upscaling active): they do the anti-aliasing themselves; your choice comes back when Upscaler is Off. With Bilinear, which smooths nothing, FXAA and SMAA still apply after it; multisampling and TAA wait until Upscaler is Off.",
            Slider::choice(graphics::AA_CHOICES.len(), |w| choice_part(graphics::aa_choice(w), graphics::AA_CHOICES.len()), |w, v| graphics::set_aa_choice(w, choice_index(v, graphics::AA_CHOICES.len())), |v| {
                graphics::AA_CHOICES[choice_index(v, graphics::AA_CHOICES.len())].0.to_string()
            }),
        ),
        Setting::slider(
            "shadow-filter",
            GROUP_GRAPHICS,
            "Shadow filtering",
            "How the edges of the sun's shadows are softened: Fast (hardware 2x2, harder edges, cheapest), Smooth (as vanilla) or Temporal (soft but grainy unless Anti-aliasing is TAA).",
            Slider::choice(
                graphics::SHADOW_FILTERS.len(),
                |w| {
                    let f = w.resource::<Graphics>().shadow_filter;
                    choice_part(graphics::SHADOW_FILTERS.iter().position(|&(_, m)| m == f).unwrap_or(1), graphics::SHADOW_FILTERS.len())
                },
                |w, v| w.resource_mut::<Graphics>().shadow_filter = graphics::SHADOW_FILTERS[choice_index(v, graphics::SHADOW_FILTERS.len())].1,
                |v| graphics::SHADOW_FILTERS[choice_index(v, graphics::SHADOW_FILTERS.len())].0.to_string(),
            ),
        ),
    ];
    for s in choices {
        add_setting(app, s);
    }
    // FSR and DLSS smooth the picture themselves; Bilinear does not, so FXAA and SMAA stay
    // the row's to choose while it draws (#2414; multisampling and TAA wait, graphics.rs).
    nvf_play::settings::set_disabled_status(app, "anti-aliasing", |w| crate::upscaler::temporal_active(w).then_some("Off (upscaling active)"));
    // Frame generation: Off, FSR 3.1 or DLSS (#2317), each choice dimmed with its reason; the
    // INI's old on/off still reads. Then DLSS's frames per real frame.
    add_choice_list(
        app,
        Setting { key: FRAME_GENERATION_KEY, group: GROUP_GRAPHICS, label: "Frame generation", note: FRAME_GENERATION_NOTE, get: |_| false, set: |_, _| {}, slider: None },
        ChoiceList {
            names: |_| crate::frame_gen::FrameGenKind::ALL.iter().map(|k| k.name().to_string()).collect(),
            chosen: frame_generation_chosen,
            pick: pick_frame_generation,
            save: frame_generation_saved,
            load: pick_frame_generation,
            ask_keep: false,
        },
    );
    nvf_play::settings::set_choice_labels(app, FRAME_GENERATION_KEY, |_, name| fnv_util::words::tr_str(name).to_string());
    nvf_play::settings::set_choice_unavailable(app, FRAME_GENERATION_KEY, frame_generation_unavailable);
    add_choice_list(
        app,
        Setting { key: FRAMES_KEY, group: GROUP_GRAPHICS, label: "Generated frames", note: FRAMES_NOTE, get: |_| false, set: |_, _| {}, slider: None },
        ChoiceList { names: |_| FRAMES_CHOICES.iter().map(|c| (*c).to_string()).collect(), chosen: frames_chosen, pick: pick_frames, save: frames_chosen, load: pick_frames, ask_keep: false },
    );
    nvf_play::settings::set_choice_unavailable(app, FRAMES_KEY, frames_unavailable);
    nvf_play::settings::set_disabled_status(app, FRAMES_KEY, frames_dimmed);
    add_setting(
        app,
        Setting {
            key: "latency-reduction",
            group: GROUP_GRAPHICS,
            label: "Latency reduction",
            note: "The graphics driver keeps the game from preparing frames ahead of the graphics card: AMD Anti-Lag on AMD cards (Windows: Adrenalin 24.9.1 or later; Linux: Mesa 26.3 or later), NVIDIA low latency on NVIDIA cards whose Vulkan driver has it. It is meant to make what you do reach the screen sooner; that has not been measured yet. The log says which of the two this graphics card uses, or why neither. Not in the original. (--latency-reduction)",
            get: |w| w.resource::<crate::anti_lag::LatencyReduction>().on,
            set: |w, on| w.resource_mut::<crate::anti_lag::LatencyReduction>().on = on,
            slider: None,
        },
    );
    nvf_play::settings::set_disabled_status(app, "latency-reduction", |w| (!crate::anti_lag::available(w)).then_some("Not on this GPU"));
    let switches: [Switch; 11] = [
        (
            "ambient-occlusion",
            "Ambient occlusion",
            "On: creases, corners and the ground under things are darkened by how enclosed they are (screen-space, from the depth). Not in the original. (--graphics)",
            |w| w.resource::<Graphics>().ambient_occlusion,
            |w, on| w.resource_mut::<Graphics>().ambient_occlusion = on,
        ),
        (
            "fine-shadows",
            "Longer sun shadows",
            "On: the sun's shadows reach half as far again as --shadow-distance. Costs a little frame rate. (--graphics)",
            |w| w.resource::<Graphics>().fine_shadows,
            |w, on| w.resource_mut::<Graphics>().fine_shadows = on,
        ),
        (
            "bloom",
            "Bloom",
            "On: light brighter than a sunlit white wall (the sun, lamps, windows) glows into its surroundings, on top of the game's own image space. (--graphics)",
            |w| w.resource::<Graphics>().bloom,
            |w, on| w.resource_mut::<Graphics>().bloom = on,
        ),
        (
            "glow",
            "Light glow (HDR)",
            "On: neon, lamps, glowing beams and fire glow softly into their surroundings, as the original's HDR glow does; how much follows the place's image space (the Strip's neon at night most). Bloom, when on, adds to it. (--graphics-only no-glow turns it off)",
            |w| w.resource::<Graphics>().glow,
            |w, on| w.resource_mut::<Graphics>().glow = on,
        ),
        (
            "eye-adaptation",
            "Eye adaptation",
            "On: dark views brighten after a moment, as the eye gets used to the dark, by the place's image space. Needs the light glow. (--graphics-only no-eye-adaptation turns it off)",
            |w| w.resource::<Graphics>().eye_adaptation,
            |w, on| w.resource_mut::<Graphics>().eye_adaptation = on,
        ),
        (
            "neon-glow",
            "Neon glow",
            "On: where the place's image space asks for a strong glow (the Strip at night) neon tubes and signs burn with a wide halo, as in the original. Needs the light glow. (--graphics-only no-neon-glow turns it off)",
            |w| w.resource::<Graphics>().neon_glow,
            |w, on| w.resource_mut::<Graphics>().neon_glow = on,
        ),
        (
            "sharpen",
            "Sharpening",
            "On: the final picture is sharpened where it has little contrast (contrast-adaptive sharpening). (--graphics)",
            |w| w.resource::<Graphics>().sharpen,
            |w, on| w.resource_mut::<Graphics>().sharpen = on,
        ),
        (
            "anisotropic",
            "Anisotropic filtering 16x",
            "On: textures seen at a slant stay sharp further (16x instead of 8x). For textures loaded from now on (the cells loaded next). (--graphics)",
            |w| w.resource::<Graphics>().anisotropic,
            |w, on| {
                w.resource_mut::<Graphics>().anisotropic = on;
                nvf_assets::texture::set_anisotropy(if on { 16 } else { 8 });
            },
        ),
        (
            "light-shafts",
            "Sun shafts",
            "On: sunlight streams past what stands in front of the sun, stronger the hazier the weather's fog. Not in the original. (--graphics)",
            |w| w.resource::<Graphics>().light_shafts,
            |w, on| w.resource_mut::<Graphics>().light_shafts = on,
        ),
        (
            "water-reflections",
            "Screen-space water reflections",
            "On: water the mirrored reflection does not reach (another height, interiors) reflects what is on the screen instead of only the sky. (--graphics)",
            |w| w.resource::<Graphics>().water_reflections,
            |w, on| w.resource_mut::<Graphics>().water_reflections = on,
        ),
        (
            "water-mirror",
            "Mirrored water reflections",
            "On: water mirrors the world above it, as in the original; the scene is drawn a second time for it while water is on screen. Off: water reflects the sky, and what is on the screen with screen-space reflections. (--graphics-only no-water-mirror turns it off)",
            |w| w.resource::<Graphics>().water_mirror,
            |w, on| w.resource_mut::<Graphics>().water_mirror = on,
        ),
    ];
    for (key, label, note, get, set) in switches {
        add_setting(app, Setting { key, group: GROUP_GRAPHICS, label, note, get, set, slider: None });
    }
}

fn register_pipeline_waits(app: &mut App) {
    for key in [
        "anti-aliasing", "shadow-filter", "upscaler", "upscaler-quality", "ambient-occlusion",
        "fine-shadows", "bloom", "glow", "eye-adaptation", "neon-glow", "sharpen",
        "light-shafts", "water-reflections", "water-mirror", "sun-shadows", "grade",
    ] {
        nvf_play::settings::wait_for_pipelines(app, key);
    }
}

/// The DLSS model row's choice: the model's name (`DlssModel::name`).
fn model_chosen(world: &World) -> String {
    world.resource::<Graphics>().dlss_model.name().to_string()
}

/// The page's and the test aids' pick: one of the five names (any case); false otherwise.
fn pick_model(world: &mut World, name: &str) -> bool {
    let Some(model) = DlssModel::named(name) else { return false };
    world.resource_mut::<Graphics>().dlss_model = model;
    true
}

/// The FSR library row's two choices; the second is shown with the library found.
const FSR_LIBRARY_BUILT_IN: &str = "Built-in";
const FSR_LIBRARY_FOLDER: &str = "Game folder";

/// The choices as shown: the built-in upscaler, and the folder's library by name
/// (`ffx_api::found`: the one loaded, the one that failed, or none).
fn fsr_library_names() -> [String; 2] {
    [FSR_LIBRARY_BUILT_IN.to_string(), format!("{FSR_LIBRARY_FOLDER} ({})", crate::ffx_api::found())]
}

/// Takes a choice by its name, with or without the library named after it.
fn pick_fsr_library(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    let folder = if name == FSR_LIBRARY_BUILT_IN.to_ascii_lowercase() {
        false
    } else if name.starts_with(&FSR_LIBRARY_FOLDER.to_ascii_lowercase()) {
        true
    } else {
        return false;
    };
    crate::ffx_api::set_enabled(folder);
    true
}

/// The FSR library row stands dimmed while FSR is not the effective upscaler
/// (`upscaler::effective`), as the DLSS model row does for DLSS, and always with
/// `NVF_FFX_API=0`. The choice is kept meanwhile.
fn fsr_library_dimmed(world: &World) -> Option<&'static str> {
    if !crate::ffx_api::allowed() {
        return Some("Built-in (NVF_FFX_API=0)");
    }
    crate::upscaler::effective(world).fsr().is_none().then_some("FSR only")
}

/// The INI's text: one of the five names, else Default (said once in the log), so a stored
/// value from elsewhere never leaves an unknown model.
fn load_model(world: &mut World, value: &str) -> bool {
    let model = DlssModel::named(value).unwrap_or_else(|| {
        let names: Vec<&str> = DlssModel::ALL.iter().map(|m| m.name()).collect();
        warn!("settings: {MODEL_KEY}={} is not one of {}: DLSS model {}", value.trim(), names.join(", "), DlssModel::Default.name());
        DlssModel::Default
    });
    world.resource_mut::<Graphics>().dlss_model = model;
    true
}

/// The DLSS model row stands dimmed while DLSS is not the effective upscaler
/// (`upscaler::effective`): FSR or off draws, DLSS waits for a restart or cannot run. The model
/// is kept meanwhile.
fn model_dimmed(world: &World) -> Option<&'static str> {
    crate::upscaler::effective(world).dlss().is_none().then_some("DLSS only")
}

/// The Upscaler row's choices by the names the INI, the log and the test aids use (the page
/// shows them by [`upscaler_label`]). Bilinear (#2414) right after Off, the cheapest first,
/// so Download DLSS… still follows DLSS.
const UPSCALERS: [(&str, Upscaler); 4] = [("Off", Upscaler::Off), ("Bilinear", Upscaler::Bilinear), ("FSR 3.1", Upscaler::Fsr), ("DLSS", Upscaler::Dlss)];

/// The quality the Upscaling mode row holds while the upscaler is Off, when the request
/// (`Graphics::upscaler`) has none: an upscaler chosen again takes it up.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeptQuality(pub UpscaleQuality);

/// What the two rows hold: the request, and the quality kept for while it is Off.
type RowsState = (UpscalerChoice, UpscaleQuality);

fn rows_state(world: &World) -> RowsState {
    (world.resource::<Graphics>().upscaler, world.get_resource::<KeptQuality>().map_or_else(UpscaleQuality::default, |k| k.0))
}

fn set_rows_state(world: &mut World, (choice, kept): RowsState) {
    world.resource_mut::<Graphics>().upscaler = choice;
    world.insert_resource(KeptQuality(kept));
}

/// The quality the quality row shows: the request's, or the one kept while it is Off.
fn shown_quality((choice, kept): RowsState) -> UpscaleQuality {
    choice.quality().unwrap_or(kept)
}

/// `upscaler` chosen, at the quality the quality row shows.
fn with_upscaler(state: RowsState, upscaler: Upscaler) -> RowsState {
    let q = shown_quality(state);
    (UpscalerChoice::new(upscaler, q), q)
}

/// The quality `q` chosen: the request's when it has one, else kept for later.
fn with_quality((choice, _): RowsState, q: UpscaleQuality) -> RowsState {
    (UpscalerChoice::new(choice.upscaler(), q), q)
}

/// The whole of `choice` chosen (the older row's choices).
fn with_choice(state: RowsState, choice: UpscalerChoice) -> RowsState {
    (choice, choice.quality().unwrap_or(shown_quality(state)))
}

/// The Upscaler row's choice of this name (any case).
fn upscaler_named(name: &str) -> Option<Upscaler> {
    UPSCALERS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name.trim())).map(|(_, u)| *u)
}

/// The quality of this name (any case).
fn quality_named(name: &str) -> Option<UpscaleQuality> {
    UpscaleQuality::ALL.into_iter().find(|q| q.name().eq_ignore_ascii_case(name.trim()))
}

/// The older row's text read as it read it: one of its eleven names, or a number written by
/// the early local slider build of the FSR row (a part of its six FSR choices, which is all
/// that build had). `None`: not one of its states.
pub(crate) fn parse_legacy(value: &str) -> Option<UpscalerChoice> {
    if let Some((_, choice)) = graphics::FSR_UPSCALING_CHOICES.iter().find(|(n, _)| n.eq_ignore_ascii_case(value.trim())) {
        return Some(*choice);
    }
    let part = value.trim().parse::<f32>().ok().filter(|v| v.is_finite() && (0.0..=1.0).contains(v))?;
    Some(UpscalerChoice::from_fsr(graphics::FSR_CHOICES[choice_index(part, graphics::FSR_CHOICES.len())].1.quality()))
}

/// The older row's value (`fsr-upscaling`) as the two rows' own values, for
/// `nvf_play::settings::set_older_split`: the Upscaler row's name and, for FSR or DLSS, the
/// quality's. So in a test run whose command line keeps both rows, both keep the stored value
/// and a save writes it, not the command line's quality beside the stored upscaler (the review
/// of 2026-11-12). Off has no quality: the quality row keeps its own. Nothing for a value that
/// is not one of the older row's (the Upscaler row then reads it as it is, and skips it).
fn split_legacy(value: &str) -> Vec<(&'static str, String)> {
    let Some(choice) = parse_legacy(value) else { return Vec::new() };
    let Some((name, _)) = UPSCALERS.iter().find(|(_, u)| *u == choice.upscaler()) else { return Vec::new() };
    let mut rows = vec![(UPSCALER_KEY, (*name).to_string())];
    if let Some(q) = choice.quality() {
        rows.push((QUALITY_KEY, q.name().to_string()));
    }
    rows
}

/// The Upscaler row's INI text applied to `state`: one of its own names (`Off`, `Bilinear`,
/// `FSR 3.1`, `DLSS`) at the quality shown, or the older `fsr-upscaling` row's text, a whole choice (the
/// row reads that key while the INI lacks its own). The start reads the INI with it too
/// ([`upscaler_after_ini`]), so the two cannot disagree.
fn upscaler_text(state: RowsState, text: &str) -> Option<RowsState> {
    match upscaler_named(text) {
        Some(upscaler) => Some(with_upscaler(state, upscaler)),
        None => parse_legacy(text).map(|choice| with_choice(state, choice)),
    }
}

/// The quality row's INI text applied to `state`.
fn quality_text(state: RowsState, text: &str) -> Option<RowsState> {
    quality_named(text).map(|q| with_quality(state, q))
}

/// The requested upscaler's name in the Upscaler row.
fn upscaler_chosen(world: &World) -> String {
    let upscaler = world.resource::<Graphics>().upscaler.upscaler();
    UPSCALERS.iter().find(|(_, u)| *u == upscaler).map_or("Off", |(name, _)| *name).to_string()
}

/// The Upscaler row's choices: Off, Bilinear, FSR 3.1, DLSS, and Download DLSS… while DLSS's files are
/// missing (`dlss_download`: picking it opens its dialog; #2250).
fn upscaler_names(world: &World) -> Vec<String> {
    let mut names: Vec<String> = UPSCALERS.iter().map(|(name, _)| (*name).to_string()).collect();
    if crate::dlss_download::offered(dlss_files_missing(world)) {
        names.push(crate::dlss_download::CHOICE.to_string());
    }
    names
}

/// The pick: an upscaler, or Download DLSS…, which opens its dialog and leaves the request
/// (`Graphics::upscaler`) as it is.
fn pick_upscaler(world: &mut World, name: &str) -> bool {
    if crate::dlss_download::is_choice(name) {
        if !crate::dlss_download::offered(dlss_files_missing(world)) {
            return false;
        }
        crate::dlss_download::ask(world);
        return true;
    }
    let Some(upscaler) = upscaler_named(name) else { return false };
    set_rows_state(world, with_upscaler(rows_state(world), upscaler));
    true
}

/// Loads the INI's text, also a DLSS choice DLSS cannot meet this run: the request is kept
/// and native draws with the reason (`upscaler::resolve`).
fn load_upscaler(world: &mut World, value: &str) -> bool {
    let Some(state) = upscaler_text(rows_state(world), value) else { return false };
    set_rows_state(world, state);
    true
}

fn quality_chosen(world: &World) -> String {
    shown_quality(rows_state(world)).name().to_string()
}

/// The quality row's pick and load (the page holds the row dimmed while the upscaler is Off;
/// the INI's text then sets the quality kept for later).
fn load_quality(world: &mut World, value: &str) -> bool {
    let Some(state) = quality_text(rows_state(world), value) else { return false };
    set_rows_state(world, state);
    true
}

/// The share of the screen's width and height (in percent) the world is drawn at in `mode` with
/// `upscaler`; `None` with the upscaler Off. FSR's and Bilinear's from FSR's ratio
/// (`fsr::FsrQuality::ratio`, what `render_size` divides by; `upscaler::bilinear::policy`); DLSS's from the render sizes DLSS 310.9.16 chose at
/// 1280x720: 1280x720, 853x480, 742x418, 640x360, 427x240 [observed, 2026-11-12:
/// the DLSS runs recorded with #2206], since DLSS's own sizes are known only once Streamline
/// answers for a view (`dlss::sizing`) and may differ with the output size and DLSS's version.
fn render_share(upscaler: Upscaler, mode: UpscaleQuality) -> Option<u32> {
    match upscaler {
        Upscaler::Off => None,
        Upscaler::Fsr | Upscaler::Bilinear => Some((100.0 / mode.ratio()).round() as u32),
        Upscaler::Dlss => Some(match mode {
            UpscaleQuality::NativeAa => 100,
            UpscaleQuality::Quality => 67,
            UpscaleQuality::Balanced => 58,
            UpscaleQuality::Performance => 50,
            UpscaleQuality::UltraPerformance => 33,
        }),
    }
}

/// The words the Upscaling mode row shows for a mode: its name in the language shown and, with
/// an upscaler asked for (`Graphics::upscaler`), the share it draws at, `Quality (67%)`
/// [design: the user's, 2026-11-12]; with the upscaler Off (the row dimmed) the name alone.
/// The INI, the log and the test aids keep the plain names.
fn quality_label(world: &World, name: &str) -> String {
    let shown = fnv_util::words::tr_str(name).to_string();
    let upscaler = world.get_resource::<Graphics>().map_or(Upscaler::Off, |g| g.upscaler.upscaler());
    match quality_named(name).and_then(|mode| render_share(upscaler, mode)) {
        Some(share) => format!("{shown} ({share}%)"),
        None => shown,
    }
}

/// The words the Upscaler row shows for a choice: `Off` and `Bilinear` in the language shown,
/// `FSR 3.1`, `DLSS`; the requested DLSS as `DLSS (restart)` while a restart is pending for it
/// (`upscaler::UpscalerStatus::restart_pending`; the row's note says why); Download DLSS…, or
/// how far its download is (`dlss_download::choice_words`).
fn upscaler_label(world: &World, name: &str) -> String {
    use fnv_util::words::tr_str;
    if crate::dlss_download::is_choice(name) {
        return crate::dlss_download::choice_words();
    }
    match upscaler_named(name) {
        Some(Upscaler::Off) => tr_str("Off").to_string(),
        Some(Upscaler::Bilinear) => tr_str("Bilinear").to_string(),
        Some(Upscaler::Dlss) => {
            let pending = world.get_resource::<crate::upscaler::UpscalerStatus>().is_some_and(|s| s.restart_pending());
            if pending && world.get_resource::<Graphics>().is_some_and(|g| g.upscaler.is_dlss()) {
                tr_str("DLSS (restart)").to_string()
            } else {
                "DLSS".to_string()
            }
        }
        Some(Upscaler::Fsr) | None => name.to_string(),
    }
}

/// Why DLSS cannot be taken this run (`upscaler::Unavailable::words`), by the status's rule.
/// Off and FSR are always in reach (FSR on a device without it draws natively, as before).
/// A stored DLSS choice still loads from the INI.
fn dlss_unavailable(world: &World) -> Option<String> {
    dlss_reason(world).map(|why| why.words().to_string())
}

/// Why DLSS cannot be taken this run, by the status's rule ([`dlss_unavailable`]'s reason).
fn dlss_reason(world: &World) -> Option<crate::upscaler::Unavailable> {
    match world.get_resource::<crate::upscaler::UpscalerStatus>() {
        Some(status) => status.dlss_unavailable(),
        None => match crate::upscaler::dlss_state_now() {
            crate::upscaler::DlssState::Unavailable(why) => Some(why),
            _ => None,
        },
    }
}

/// DLSS is out of reach only because its files are missing: what Download DLSS… mends.
fn dlss_files_missing(world: &World) -> bool {
    dlss_reason(world) == Some(crate::upscaler::Unavailable::FilesMissing)
}

fn upscaler_unavailable(world: &World, name: &str) -> Option<String> {
    (upscaler_named(name)? == Upscaler::Dlss).then(|| dlss_unavailable(world)).flatten()
}

/// The older row's name for the request now (the test aid's `settings: fsr-upscaling <name>`).
fn legacy_chosen(world: &World) -> String {
    let requested = world.resource::<Graphics>().upscaler;
    // Bilinear (#2414) is none of the older row's choices: named as the log names it.
    graphics::FSR_UPSCALING_CHOICES.iter().find(|(_, choice)| *choice == requested).map_or_else(|| requested.describe(), |(name, _)| (*name).to_string())
}

/// The test aid's `fsr-upscaling=<name>`: the older row's whole choice, set on both rows.
fn pick_legacy(world: &mut World, name: &str) -> bool {
    let Some((_, choice)) = graphics::FSR_UPSCALING_CHOICES.iter().find(|(n, _)| n.eq_ignore_ascii_case(name.trim())) else { return false };
    set_rows_state(world, with_choice(rows_state(world), *choice));
    true
}

fn legacy_unavailable(world: &World, name: &str) -> Option<String> {
    let (_, choice) = graphics::FSR_UPSCALING_CHOICES.iter().find(|(n, _)| n.eq_ignore_ascii_case(name.trim()))?;
    choice.is_dlss().then(|| dlss_unavailable(world)).flatten()
}

/// The INI's texts for the two rows, as `nvf_play::settings::apply_stored` finds them:
/// `upscaler` and `upscaler-quality`, the older `fsr-upscaling` split into them first where
/// the INI lacks `upscaler` ([`split_legacy`], `nvf_play::settings::with_older_split`), else
/// read as it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct UpscalerIni {
    pub upscaler: Option<String>,
    pub quality: Option<String>,
}

impl UpscalerIni {
    /// From an INI's text.
    pub(crate) fn read(text: &str) -> Self {
        let text = nvf_play::settings::with_older_split(text, UPSCALER_KEY, LEGACY_KEY, split_legacy);
        let get = |key: &str| fnv_install::ini_get(&text, nvf_play::settings::SECTION, key);
        UpscalerIni { upscaler: get(UPSCALER_KEY).or_else(|| get(LEGACY_KEY)), quality: get(QUALITY_KEY) }
    }
}

/// The upscaler this run starts with, decided before the renderer exists (the DLSS loader runs
/// for a DLSS choice; `main`): the command line's choice, then the INI's two rows by
/// `nvf_play::settings::apply_stored`'s order (Upscaler, then quality) and precedence
/// (`after_ini`), each read with its row's own rule ([`upscaler_text`], [`quality_text`]).
/// `given`: the switches this run was given, `None` when it has no settings page (not
/// `--play`: the INI is not applied, the command line decides).
pub(crate) fn upscaler_after_ini(command_line: UpscalerChoice, ini: Option<&UpscalerIni>, given: Option<&nvf_play::settings::GivenSwitches>) -> UpscalerChoice {
    let Some(given) = given else { return command_line };
    // The quality kept while Off starts as the rows' (KeptQuality's default).
    let start = (command_line, UpscaleQuality::default());
    let stored = ini.filter(|ini| ini.upscaler.is_some() || ini.quality.is_some()).map(|ini| {
        let after_upscaler = ini.upscaler.as_deref().and_then(|t| upscaler_text(start, t.trim())).unwrap_or(start);
        ini.quality.as_deref().and_then(|t| quality_text(after_upscaler, t.trim())).unwrap_or(after_upscaler).0
    });
    nvf_play::settings::after_ini(command_line, stored, given, UPSCALER_NOTE)
}

/// The Frame generation row's key and note (#2317). The note names the switch, so a test run
/// that gives it keeps the row at the command line's choice over the INI.
pub(crate) const FRAME_GENERATION_KEY: &str = "frame-generation";
pub(crate) const FRAME_GENERATION_NOTE: &str = "Frames made between the real ones, so motion looks smoother; what you do reaches the screen a little later. FSR 3.1: one frame between each two real frames; needs FSR 3.1 upscaling (any mode; not with DLSS) and vertical sync or variable refresh, and real frames are held to just under half the refresh rate. DLSS: NVIDIA DLSS Frame Generation, one to three frames for each real frame (Generated frames, below); needs an upscaler (FSR 3.1 or DLSS), Display sync Off, an NVIDIA RTX 40 series or newer graphics card, Windows' hardware-accelerated GPU scheduling and NVIDIA's frame generation files in the dlss folder beside nvforever.exe; frames are generated only while the game's window is in front. Latency reduction turns on with either. The HUD and menus are not generated; menus and loading screens show real frames only. Best from 60 real frames a second. Not in the original. (--frame-generation)";

/// The Generated frames row (#2317): DLSS Frame Generation's frames per real frame
/// (`frame_gen::DlssgFrames`), dimmed unless DLSS is the Frame generation choice; 3x and 4x only
/// where the GPU offers them (`dlss::frame_generation_max`, `DLSSGState::numFramesToGenerateMax`).
pub(crate) const FRAMES_KEY: &str = "frame-generation-frames";
pub(crate) const FRAMES_NOTE: &str = "How many frames DLSS frame generation makes for each real frame: 2x one, 3x two, 4x three. More looks smoother and costs a little more latency and graphics memory; 3x and 4x only where the graphics card offers them (RTX 50 series). Takes effect at once. Only with DLSS frame generation. Not in the original.";
const FRAMES_CHOICES: [&str; 3] = ["2x", "3x", "4x"];

/// The frames generated per real frame of a Generated frames choice (`2x` is one).
fn frames_named(name: &str) -> Option<u32> {
    FRAMES_CHOICES.iter().position(|c| c.eq_ignore_ascii_case(name.trim())).map(|i| i as u32 + 1)
}

fn frames_chosen(world: &World) -> String {
    FRAMES_CHOICES[(world.resource::<crate::frame_gen::DlssgFrames>().0.clamp(1, 3) - 1) as usize].to_string()
}

fn pick_frames(world: &mut World, name: &str) -> bool {
    let Some(frames) = frames_named(name) else { return false };
    world.resource_mut::<crate::frame_gen::DlssgFrames>().0 = frames;
    true
}

/// A Generated frames choice the GPU does not offer (once its limit is known).
fn frames_unavailable(_world: &World, name: &str) -> Option<String> {
    let max = crate::dlss::frame_generation_max();
    frames_named(name).filter(|&n| max != 0 && n > max).map(|_| "Not on this GPU".to_string())
}

/// The Generated frames row stands dimmed unless DLSS is the Frame generation choice, or when
/// the GPU makes one frame at most.
fn frames_dimmed(world: &World) -> Option<&'static str> {
    if world.resource::<crate::frame_gen::FrameGeneration>().kind != crate::frame_gen::FrameGenKind::Dlss {
        Some("Needs DLSS frame generation")
    } else if crate::dlss::frame_generation_max() == 1 {
        Some("Not on this GPU")
    } else {
        None
    }
}

fn frame_generation_chosen(world: &World) -> String {
    world.resource::<crate::frame_gen::FrameGeneration>().kind.name().to_string()
}

fn frame_generation_saved(world: &World) -> String {
    world.resource::<crate::frame_gen::FrameGeneration>().kind.stored().to_string()
}

/// A Frame generation choice by its name or INI text (also the row's old on/off).
fn pick_frame_generation(world: &mut World, name: &str) -> bool {
    let Some(kind) = crate::frame_gen::FrameGenKind::parse(name) else { return false };
    world.resource_mut::<crate::frame_gen::FrameGeneration>().kind = kind;
    true
}

/// Why a Frame generation choice cannot run now: FSR 3.1's reasons (`frame_gen::unavailable_reason`)
/// or DLSS's (`frame_gen::dlss_unavailable_reason`). Off is always in reach.
fn frame_generation_unavailable(world: &World, name: &str) -> Option<String> {
    match crate::frame_gen::FrameGenKind::parse(name)? {
        crate::frame_gen::FrameGenKind::Off => None,
        crate::frame_gen::FrameGenKind::Fsr => crate::frame_gen::unavailable_reason(world).map(str::to_string),
        crate::frame_gen::FrameGenKind::Dlss => crate::frame_gen::dlss_unavailable_reason(world).map(str::to_string),
    }
}

/// The Frame generation choice this run starts with, decided before the renderer exists (DLSS
/// Frame Generation stays loaded from the start only for DLSS; `main`): the command line's,
/// then the INI's row by `nvf_play::settings::after_ini`'s precedence. `given`: `None` without
/// a settings page (not `--play`: the command line decides).
pub(crate) fn frame_generation_after_ini(command_line: crate::frame_gen::FrameGenKind, ini: Option<&str>, given: Option<&nvf_play::settings::GivenSwitches>) -> crate::frame_gen::FrameGenKind {
    let Some(given) = given else { return command_line };
    let stored = ini.and_then(|text| fnv_install::ini_get(text, nvf_play::settings::SECTION, FRAME_GENERATION_KEY)).and_then(|v| crate::frame_gen::FrameGenKind::parse(&v));
    nvf_play::settings::after_ini(command_line, stored, given, FRAME_GENERATION_NOTE)
}

/// Distant LOD on or off now; indoors there is none either way, and the door out goes
/// by `Args::no_lod` (`world::travel`).
fn set_lod(world: &mut World, on: bool) {
    world.resource_mut::<Args>().no_lod = !on;
    let outdoors = world.resource::<Streamer>().interior.is_none();
    world.resource_scope(|world, mut lod: Mut<LodState>| {
        lod.set_enabled(&mut world.commands(), on && outdoors);
    });
    world.flush();
}

fn set_overlay(world: &mut World, on: bool) {
    world.resource_mut::<Args>().no_overlay = !on;
    let mut overlays = world.query_filtered::<&mut Visibility, With<Overlay>>();
    for mut v in overlays.iter_mut(world) {
        *v = if on { Visibility::Inherited } else { Visibility::Hidden };
    }
}

#[cfg(test)]
mod fsr_settings_tests {
    use super::*;

    #[test]
    fn renderer_registration_covers_aa_ao_fsr_and_defaults_without_latching_no_ops() {
        use nvf_play::{graphics_resume::GraphicsResume, pause::Paused, settings, PlayState};
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla))
            .insert_resource(graphics::MsaaSetting(Msaa::Off)).init_resource::<GraphicsResume>()
            .insert_resource(Paused(true)).insert_resource(State::new(PlayState::Menu));
        register_graphics(&mut app);
        register_pipeline_waits(&mut app);
        let rows = app.world().resource::<settings::Settings>();
        let aa = rows.index("anti-aliasing").unwrap();
        let ao = rows.index("ambient-occlusion").unwrap();
        let fsr = rows.index(UPSCALER_KEY).unwrap();
        let world = app.world_mut();
        settings::set_slider(world, aa, 0.0);
        settings::restore(world, ao, 0.0);
        assert!(!settings::pick(world, fsr, "Off"));
        assert!(world.resource_mut::<GraphicsResume>().request(0.0));
        settings::set_slider(world, aa, 1.0);
        assert!(!world.resource_mut::<GraphicsResume>().request(0.0));
        world.resource_mut::<GraphicsResume>().reset();
        settings::restore(world, aa, 0.0);
        assert!(!world.resource_mut::<GraphicsResume>().request(0.0));
        world.resource_mut::<GraphicsResume>().reset();
        settings::flip(world, ao);
        assert!(!world.resource_mut::<GraphicsResume>().request(0.0));
        world.resource_mut::<GraphicsResume>().reset();
        settings::pick(world, fsr, "FSR 3.1");
        assert!(!world.resource_mut::<GraphicsResume>().request(0.0));
        world.resource_mut::<GraphicsResume>().reset();
        settings::restore(world, fsr, 0.0);
        assert!(!world.resource_mut::<GraphicsResume>().request(0.0), "Defaults also restores named choices");
    }

    #[test]
    fn native_aa_row_is_off_and_locked_for_every_supported_fsr_mode_then_restores_the_saved_choice() {
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla))
            .insert_resource(graphics::MsaaSetting(Msaa::Sample8))
            .insert_resource(crate::fsr::FsrSupport(true));
        register_graphics(&mut app);
        let row = app.world().resource::<nvf_play::settings::Settings>().index("anti-aliasing").unwrap();
        for (_, mode) in graphics::FSR_CHOICES.into_iter().filter(|(_, m)| m.quality().is_some()) {
            app.world_mut().resource_mut::<Graphics>().set_fsr(mode.quality());
            let settings = app.world().resource::<nvf_play::settings::Settings>();
            assert_eq!(settings.disabled_status(app.world(), row), Some("Off (upscaling active)"));
            let saved = settings.list()[row].value(app.world());
            assert_eq!(saved, choice_part(3, graphics::AA_CHOICES.len()));
            assert!(nvf_play::settings::set_slider(app.world_mut(), row, 0.0).is_none());
            assert!(nvf_play::settings::step_slider(app.world_mut(), row, 1).is_none());
            assert_eq!(graphics::aa_choice(app.world()), 3, "disabled UI must preserve saved 8x");
            app.world_mut().resource_mut::<Graphics>().set_fsr(None);
            let settings = app.world().resource::<nvf_play::settings::Settings>();
            assert_eq!(settings.disabled_status(app.world(), row), None);
            assert_eq!(settings.list()[row].slider.unwrap().text(saved), "8x");
        }
        app.world_mut().resource_mut::<crate::fsr::FsrSupport>().0 = false;
        app.world_mut().resource_mut::<Graphics>().set_fsr(Some(crate::fsr::FsrQuality::NativeAa));
        assert_eq!(app.world().resource::<nvf_play::settings::Settings>().disabled_status(app.world(), row), None, "unsupported FSR leaves native AA editable");
        assert!(nvf_play::settings::set_slider(app.world_mut(), row, 0.0).is_some());
        assert_eq!(graphics::aa_choice(app.world()), 0);
    }

    #[test]
    fn the_fsr_library_row_switches_between_the_built_in_upscaler_and_the_folders() {
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla));
        register_graphics(&mut app);
        let list = {
            let settings = app.world().resource::<nvf_play::settings::Settings>();
            settings.choices(settings.index("fsr-library").unwrap()).expect("a dropdown")
        };
        // No library is beside the test's executable, and none was loaded.
        assert_eq!((list.names)(app.world()), ["Built-in", "Game folder (none found)"]);
        assert_eq!((list.chosen)(app.world()), "Game folder (none found)", "the folder's library is used by default");
        assert!((list.pick)(app.world_mut(), "Built-in"));
        assert!(!crate::ffx_api::enabled());
        assert_eq!(((list.chosen)(app.world()), (list.save)(app.world())), ("Built-in".to_string(), "Built-in".to_string()));
        // As the page gives it (with the library named) and as the INI keeps it.
        assert!((list.pick)(app.world_mut(), "Game folder (amd_fidelityfx_vk.dll)"));
        assert!(crate::ffx_api::enabled());
        assert_eq!((list.save)(app.world()), "Game folder");
        assert!((list.load)(app.world_mut(), "built-in") && !crate::ffx_api::enabled());
        assert!((list.load)(app.world_mut(), "Game folder") && crate::ffx_api::enabled());
        assert!(!(list.pick)(app.world_mut(), "Somewhere else"));
        assert!(crate::ffx_api::enabled(), "an unknown name changes nothing");
    }

    #[test]
    fn native_aa_row_is_locked_while_dlss_draws_and_follows_the_effective_upscaler() {
        use crate::upscaler::{DlssState, Session, UpscaleQuality, UpscalerStatus};
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla))
            .insert_resource(graphics::MsaaSetting(Msaa::Sample4))
            .insert_resource(crate::fsr::FsrSupport(true));
        register_graphics(&mut app);
        let row = app.world().resource::<nvf_play::settings::Settings>().index("anti-aliasing").unwrap();
        let ready = Session { streamline_loaded: true, dlss: DlssState::Ready, fsr: None };
        let mut status = UpscalerStatus::new(ready);
        status.update(UpscalerChoice::Dlss(UpscaleQuality::Quality));
        app.insert_resource(status);
        app.world_mut().resource_mut::<Graphics>().upscaler = UpscalerChoice::Dlss(UpscaleQuality::Quality);
        let locked = |app: &App| app.world().resource::<nvf_play::settings::Settings>().disabled_status(app.world(), row);
        assert_eq!(locked(&app), Some("Off (upscaling active)"), "DLSS draws");
        assert!(nvf_play::settings::set_slider(app.world_mut(), row, 0.0).is_none());
        // Off asked for: native at once (Streamline loaded), the row editable again.
        app.world_mut().resource_mut::<Graphics>().upscaler = UpscalerChoice::Off;
        app.world_mut().resource_mut::<UpscalerStatus>().update(UpscalerChoice::Off);
        assert_eq!(locked(&app), None);
        assert_eq!(graphics::aa_choice(app.world()), 2, "the saved 4x is back");
        // DLSS asked for in a session that started without Streamline (the files there now):
        // what drew, native, stays until the restart, so the row stays editable.
        let stock = Session { streamline_loaded: false, dlss: DlssState::NotLoaded, fsr: None };
        let mut status = UpscalerStatus::new(stock);
        status.update(UpscalerChoice::Off);
        status.update(UpscalerChoice::Dlss(UpscaleQuality::Quality));
        assert!(status.restart_pending());
        app.insert_resource(status);
        app.world_mut().resource_mut::<Graphics>().upscaler = UpscalerChoice::Dlss(UpscaleQuality::Quality);
        assert_eq!(locked(&app), None);
    }

    /// Bilinear (#2414) is in reach in any session (no FSR, no DLSS), picked on the Upscaler row
    /// and switched at once. It leaves the anti-aliasing row editable, unlike FSR and DLSS: FXAA
    /// and SMAA apply after it (multisampling and TAA wait, `graphics::apply`); Off keeps the
    /// row's choice.
    #[test]
    fn the_aa_row_stays_editable_while_bilinear_draws_on_a_device_without_fsr_or_dlss() {
        use crate::upscaler::{DlssState, Session, Unavailable, UpscalerStatus};
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla))
            .insert_resource(graphics::MsaaSetting(Msaa::Sample4))
            .insert_resource(crate::fsr::FsrSupport(false));
        register_graphics(&mut app);
        let settings = app.world().resource::<nvf_play::settings::Settings>();
        let (aa, u, q) = (settings.index("anti-aliasing").unwrap(), settings.index(UPSCALER_KEY).unwrap(), settings.index(QUALITY_KEY).unwrap());
        let neither = Session { streamline_loaded: false, dlss: DlssState::Unavailable(Unavailable::NotVulkan), fsr: Some(Unavailable::FsrUnsupported) };
        app.insert_resource(UpscalerStatus::new(neither));
        let locked = |app: &App| app.world().resource::<nvf_play::settings::Settings>().disabled_status(app.world(), aa);
        let follow = |app: &mut App| {
            let requested = app.world().resource::<Graphics>().upscaler;
            app.world_mut().resource_mut::<UpscalerStatus>().update(requested);
        };
        assert!(nvf_play::settings::pick(app.world_mut(), u, "Bilinear"), "in reach without FSR or DLSS");
        assert!(nvf_play::settings::pick(app.world_mut(), q, "Performance"));
        follow(&mut app);
        assert_eq!(app.world().resource::<Graphics>().upscaler, UpscalerChoice::Bilinear(UpscaleQuality::Performance));
        assert_eq!(app.world().resource::<UpscalerStatus>().effective(), UpscalerChoice::Bilinear(UpscaleQuality::Performance), "draws at once");
        assert_eq!(locked(&app), None, "FXAA and SMAA still apply with Bilinear");
        assert_eq!(graphics::aa_choice(app.world()), 2, "the saved 4x, kept for Off");
        let fxaa = graphics::AA_CHOICES.iter().position(|(n, ..)| *n == "FXAA").unwrap();
        assert!(nvf_play::settings::set_slider(app.world_mut(), aa, choice_part(fxaa, graphics::AA_CHOICES.len())).is_some());
        assert_eq!(app.world().resource::<Graphics>().screen_aa, graphics::ScreenAa::Fxaa);
        assert_eq!(app.world().resource::<nvf_play::settings::Settings>().choice_label(app.world(), u, "Bilinear"), "Bilinear");
        assert_eq!(legacy_chosen(app.world()), "Bilinear Performance", "the test aid's older row names it");
        assert!(nvf_play::settings::pick(app.world_mut(), u, "Off"));
        follow(&mut app);
        assert_eq!(locked(&app), None);
        assert_eq!(graphics::aa_choice(app.world()), fxaa, "the choice made meanwhile stays");
        // FSR or DLSS, by contrast, lock the row (they smooth the picture themselves).
        app.insert_resource(UpscalerStatus::new(Session { streamline_loaded: true, dlss: DlssState::Ready, fsr: None }));
        assert!(nvf_play::settings::pick(app.world_mut(), u, "DLSS"));
        follow(&mut app);
        assert_eq!(locked(&app), Some("Off (upscaling active)"));
    }

    use crate::upscaler::{DlssState, Session, Unavailable, UpscalerStatus};
    use nvf_play::settings::{apply_stored, GivenSwitches, Settings, SettingsFile};
    use UpscaleQuality::*;

    /// The graphics rows registered, without an INI.
    fn rows_app() -> App {
        let mut app = App::new();
        app.insert_resource(Graphics::preset(graphics::Preset::Vanilla));
        register_graphics(&mut app);
        app
    }

    fn row(app: &App, key: &str) -> usize {
        app.world().resource::<Settings>().index(key).unwrap()
    }

    fn requested(app: &App) -> UpscalerChoice {
        app.world().resource::<Graphics>().upscaler
    }

    /// The two rows' chosen names, as the page and the INI have them.
    fn chosen(app: &App) -> (String, String) {
        (upscaler_chosen(app.world()), quality_chosen(app.world()))
    }

    const READY: Session = Session { streamline_loaded: true, dlss: DlssState::Ready, fsr: None };
    const STOCK: Session = Session { streamline_loaded: false, dlss: DlssState::NotLoaded, fsr: None };

    #[test]
    fn the_upscaler_and_its_quality_are_two_named_dropdowns_that_round_trip_every_choice() {
        let mut app = rows_app();
        app.insert_resource(UpscalerStatus::new(READY));
        let (u, q) = (row(&app, UPSCALER_KEY), row(&app, QUALITY_KEY));
        let settings = app.world().resource::<Settings>();
        assert_eq!(q, u + 1, "the quality row right after the Upscaler row");
        assert_eq!((settings.list()[u].label, settings.list()[q].label), ("Upscaler", "Upscaling mode"));
        assert!(settings.index(LEGACY_KEY).is_none(), "the older row is no row of its own");
        let upscalers = settings.choices(u).expect("a native dropdown");
        let qualities = settings.choices(q).expect("a native dropdown");
        assert_eq!((upscalers.names)(app.world()), ["Off", "Bilinear", "FSR 3.1", "DLSS"]);
        assert_eq!((qualities.names)(app.world()), ["Native AA", "Quality", "Balanced", "Performance", "Ultra Performance"]);
        assert_eq!(settings.older_key(u), Some(LEGACY_KEY));
        assert_eq!(chosen(&app), ("Off".to_string(), "Quality".to_string()), "off, the quality kept at Quality");
        for upscaler in [Upscaler::Bilinear, Upscaler::Fsr, Upscaler::Dlss] {
            let name = upscaler.name();
            assert!(nvf_play::settings::pick(app.world_mut(), u, name), "{name}");
            for quality in UpscaleQuality::ALL {
                nvf_play::settings::pick(app.world_mut(), q, quality.name());
                let want = UpscalerChoice::new(upscaler, quality);
                assert_eq!(requested(&app), want);
                // What the INI keeps loads back into the same request, from any other.
                let saved = ((upscalers.save)(app.world()), (qualities.save)(app.world()));
                app.world_mut().resource_mut::<Graphics>().upscaler = UpscalerChoice::Off;
                assert!((upscalers.load)(app.world_mut(), &saved.0) && (qualities.load)(app.world_mut(), &saved.1));
                assert_eq!(requested(&app), want, "{saved:?}");
            }
        }
        assert!((upscalers.pick)(app.world_mut(), "fsr 3.1"), "any case");
        assert!((qualities.pick)(app.world_mut(), "ultra performance"));
        assert_eq!(requested(&app), UpscalerChoice::Fsr(UltraPerformance));
        assert!(!pick_upscaler(app.world_mut(), "XeSS") && !load_quality(app.world_mut(), "Extreme") && !load_upscaler(app.world_mut(), "NaN"));
        assert_eq!(requested(&app), UpscalerChoice::Fsr(UltraPerformance), "nothing taken");
    }

    /// The Upscaling mode row's words follow the upscaler asked for: FSR's shares from its
    /// ratios, DLSS's from the sizes it chose, none while Off; the names the INI and the test
    /// aids use stay the plain ones.
    #[test]
    fn the_mode_row_shows_the_share_each_upscaler_draws_at() {
        let mut app = rows_app();
        app.insert_resource(UpscalerStatus::new(READY));
        let q = row(&app, QUALITY_KEY);
        let labels = |app: &App| -> Vec<String> {
            let settings = app.world().resource::<Settings>();
            UpscaleQuality::ALL.iter().map(|m| settings.choice_label(app.world(), q, m.name())).collect()
        };
        assert_eq!(labels(&app), ["Native AA", "Quality", "Balanced", "Performance", "Ultra Performance"], "Off: the plain names");
        app.world_mut().resource_mut::<Graphics>().upscaler = UpscalerChoice::Dlss(Quality);
        assert_eq!(labels(&app), ["Native AA (100%)", "Quality (67%)", "Balanced (58%)", "Performance (50%)", "Ultra Performance (33%)"]);
        app.world_mut().resource_mut::<Graphics>().upscaler = UpscalerChoice::Fsr(Balanced);
        assert_eq!(labels(&app), ["Native AA (100%)", "Quality (67%)", "Balanced (59%)", "Performance (50%)", "Ultra Performance (33%)"]);
        app.world_mut().resource_mut::<Graphics>().upscaler = UpscalerChoice::Bilinear(Performance);
        assert_eq!(labels(&app), ["Native AA (100%)", "Quality (67%)", "Balanced (59%)", "Performance (50%)", "Ultra Performance (33%)"], "Bilinear draws FSR's shares");
        for mode in UpscaleQuality::ALL {
            assert_eq!(render_share(Upscaler::Fsr, mode), Some((100.0 / mode.ratio()).round() as u32), "{mode:?}: FSR's own ratio");
            assert_eq!(render_share(Upscaler::Bilinear, mode), render_share(Upscaler::Fsr, mode), "{mode:?}");
            // Bilinear's share is what its policy draws at, to the percent.
            let bilinear = crate::upscaler::bilinear::policy(mode, UVec2::new(2000, 1000)).render_size;
            assert_eq!(render_share(Upscaler::Bilinear, mode), Some((bilinear.x as f32 / 20.0).round() as u32), "{mode:?}");
            // FSR's share is what render_size draws at, to the percent.
            let drawn = mode.render_size(UVec2::new(2000, 1000));
            assert_eq!(render_share(Upscaler::Fsr, mode), Some((drawn.x as f32 / 20.0).round() as u32), "{mode:?}");
            assert_eq!(render_share(Upscaler::Off, mode), None);
        }
        // DLSS's from the sizes it chose at 1280x720 (853x480 for Quality, 742x418 for Balanced …).
        for (mode, width) in [(NativeAa, 1280), (Quality, 853), (Balanced, 742), (Performance, 640), (UltraPerformance, 427)] {
            assert_eq!(render_share(Upscaler::Dlss, mode), Some((width as f32 * 100.0 / 1280.0).round() as u32), "{mode:?}");
        }
        // The names stay what the INI and the test aids use.
        // (Bilinear(Performance) is chosen above, so take another name: a choice already made is not taken.)
        assert!(nvf_play::settings::pick(app.world_mut(), q, "Quality"));
        assert_eq!(quality_chosen(app.world()), "Quality");
        assert!(!nvf_play::settings::pick(app.world_mut(), q, "Balanced (59%)"), "the words are not a name");
    }

    /// The quality row stands dimmed with a reason while the upscaler is Off, as the other rows
    /// that wait on a switch do; the quality is kept meanwhile, so the next upscaler takes it up.
    #[test]
    fn the_quality_row_is_dimmed_while_the_upscaler_is_off_and_keeps_its_quality() {
        let mut app = rows_app();
        app.insert_resource(UpscalerStatus::new(READY));
        let (u, q) = (row(&app, UPSCALER_KEY), row(&app, QUALITY_KEY));
        let status = |app: &App| app.world().resource::<Settings>().disabled_status(app.world(), q);
        assert_eq!(status(&app), Some("Needs an upscaler"));
        assert!(!nvf_play::settings::pick(app.world_mut(), q, "Balanced"), "dimmed: not taken from the page or a test aid");
        assert!(nvf_play::settings::pick(app.world_mut(), u, "FSR 3.1"));
        assert_eq!(status(&app), None);
        assert!(nvf_play::settings::pick(app.world_mut(), q, "Balanced"));
        assert!(nvf_play::settings::pick(app.world_mut(), u, "Off"));
        assert_eq!((requested(&app), status(&app)), (UpscalerChoice::Off, Some("Needs an upscaler")));
        assert_eq!(quality_chosen(app.world()), "Balanced", "kept while Off");
        assert!(nvf_play::settings::pick(app.world_mut(), u, "FSR 3.1"));
        assert_eq!(requested(&app), UpscalerChoice::Fsr(Balanced), "taken up again");
        // The INI's text loads into the kept quality while Off (loading is not the page).
        assert!(nvf_play::settings::pick(app.world_mut(), u, "Off"));
        assert!(load_quality(app.world_mut(), "Performance"));
        assert_eq!(requested(&app), UpscalerChoice::Off);
        assert!(nvf_play::settings::pick(app.world_mut(), u, "DLSS"));
        assert_eq!(requested(&app), UpscalerChoice::Dlss(Performance));
    }

    #[test]
    fn choices_are_shown_in_short_words_and_dlss_says_restart_only_while_it_waits() {
        let mut app = rows_app();
        let (u, q) = (row(&app, UPSCALER_KEY), row(&app, QUALITY_KEY));
        let label = |app: &App, i: usize, name: &str| app.world().resource::<Settings>().choice_label(app.world(), i, name);
        assert_eq!([label(&app, u, "Off"), label(&app, u, "Bilinear"), label(&app, u, "FSR 3.1"), label(&app, u, "DLSS")], ["Off", "Bilinear", "FSR 3.1", "DLSS"]);
        for quality in UpscaleQuality::ALL {
            assert_eq!(label(&app, q, quality.name()), quality.name(), "Off: the plain names");
        }
        // DLSS asked for in a session that started without Streamline, the files there now.
        app.insert_resource(UpscalerStatus::new(STOCK));
        assert!(nvf_play::settings::pick(app.world_mut(), u, "DLSS"));
        let requested = requested(&app);
        app.world_mut().resource_mut::<UpscalerStatus>().update(requested);
        assert!(app.world().resource::<UpscalerStatus>().restart_pending());
        assert_eq!(label(&app, u, "DLSS"), "DLSS (restart)");
        assert_eq!(label(&app, u, "FSR 3.1"), "FSR 3.1", "only DLSS says so");
        assert!("DLSS (restart)".len() <= "Ultra Performance".len(), "short enough for the box");
        // With Streamline loaded nothing waits.
        app.insert_resource(UpscalerStatus::new(READY));
        app.world_mut().resource_mut::<UpscalerStatus>().update(requested);
        assert_eq!(label(&app, u, "DLSS"), "DLSS");
    }

    /// Download DLSS… (#2250) is a choice of the Upscaler row only while DLSS's files are what is
    /// missing (on Windows, where that reason arises): picking it asks for its dialog and leaves
    /// the request as it is.
    #[test]
    fn download_dlss_is_a_choice_only_while_the_files_are_missing() {
        use crate::dlss_download::{Asked, CHOICE};
        let missing = Session { streamline_loaded: false, dlss: DlssState::Unavailable(Unavailable::FilesMissing), fsr: None };
        let mut app = rows_app();
        app.insert_resource(UpscalerStatus::new(missing));
        let u = row(&app, UPSCALER_KEY);
        let names = |app: &App| (app.world().resource::<Settings>().choices(u).unwrap().names)(app.world());
        if !cfg!(windows) {
            assert_eq!(names(&app), ["Off", "Bilinear", "FSR 3.1", "DLSS"], "DLSS, and its download, are Windows only");
            assert!(!pick_upscaler(app.world_mut(), CHOICE));
            return;
        }
        assert_eq!(names(&app), ["Off", "Bilinear", "FSR 3.1", "DLSS", CHOICE]);
        let settings = app.world().resource::<Settings>();
        assert_eq!(settings.choice_unavailable(app.world(), u, CHOICE), None, "in reach while DLSS is not");
        assert_eq!(settings.choice_unavailable(app.world(), u, "DLSS").as_deref(), Some(Unavailable::FilesMissing.words()));
        assert_eq!(settings.choice_label(app.world(), u, CHOICE), "Download DLSS…");
        assert!(nvf_play::settings::pick(app.world_mut(), u, CHOICE));
        assert!(app.world().contains_resource::<Asked>(), "the dialog asked for");
        assert_eq!(requested(&app), UpscalerChoice::Off, "the request unchanged");
        assert_eq!(upscaler_chosen(app.world()), "Off");
        // Any other reason, or DLSS in reach: no download offered.
        for session in [
            READY,
            STOCK,
            Session { streamline_loaded: false, dlss: DlssState::Unavailable(Unavailable::NotVulkan), fsr: None },
            Session { streamline_loaded: false, dlss: DlssState::Unavailable(Unavailable::UpdaterInFolder), fsr: None },
            Session { streamline_loaded: false, dlss: DlssState::Unavailable(Unavailable::GpuNotSupported), fsr: None },
        ] {
            let mut app = rows_app();
            app.insert_resource(UpscalerStatus::new(session));
            assert_eq!(names(&app), ["Off", "Bilinear", "FSR 3.1", "DLSS"], "{session:?}");
            assert!(!pick_upscaler(app.world_mut(), CHOICE), "{session:?}");
            assert!(!app.world().contains_resource::<Asked>());
        }
    }

    #[test]
    fn dlss_is_out_of_reach_with_the_reason_but_a_stored_one_loads() {
        for why in [Unavailable::FilesMissing, Unavailable::NotWindows, Unavailable::NotVulkan, Unavailable::GpuNotSupported, Unavailable::UpdaterInFolder, Unavailable::EvaluateFailed] {
            let mut app = rows_app();
            app.insert_resource(UpscalerStatus::new(Session { streamline_loaded: false, dlss: DlssState::Unavailable(why), fsr: None }));
            let (u, q) = (row(&app, UPSCALER_KEY), row(&app, QUALITY_KEY));
            let settings = app.world().resource::<Settings>();
            assert_eq!(settings.choice_unavailable(app.world(), u, "DLSS").as_deref(), Some(why.words()), "{why:?}");
            assert_eq!(settings.choice_unavailable(app.world(), u, "Off"), None);
            assert_eq!(settings.choice_unavailable(app.world(), u, "FSR 3.1"), None);
            for quality in UpscaleQuality::ALL {
                assert_eq!(settings.choice_unavailable(app.world(), q, quality.name()), None, "the qualities are FSR's too");
            }
            assert!(!nvf_play::settings::pick(app.world_mut(), u, "DLSS"), "{why:?}: not taken from the page or a test aid");
            assert!(!nvf_play::settings::pick_alias(app.world_mut(), LEGACY_KEY, "DLSS Quality"), "{why:?}: nor by the older key");
            assert_eq!(requested(&app), UpscalerChoice::Off);
            assert!(nvf_play::settings::pick(app.world_mut(), u, "FSR 3.1"));
            // The INI's text still loads: the request survives the run.
            assert!(load_upscaler(app.world_mut(), "DLSS"));
            assert_eq!(requested(&app), UpscalerChoice::Dlss(Quality));
            assert!(load_upscaler(app.world_mut(), "DLSS Performance"), "the older row's text too");
            assert_eq!(requested(&app), UpscalerChoice::Dlss(Performance));
        }
        // Ready, or not loaded with the files there (a restart brings it): in reach.
        for session in [READY, STOCK] {
            let mut app = rows_app();
            app.insert_resource(UpscalerStatus::new(session));
            let u = row(&app, UPSCALER_KEY);
            assert_eq!(app.world().resource::<Settings>().choice_unavailable(app.world(), u, "DLSS"), None, "{session:?}");
        }
    }

    /// DLSS given up on (`EvaluateFailed`): DLSS is out of reach with the reason, the stored
    /// DLSS request stays the request and the camera draws natively; FSR and off follow.
    #[test]
    fn dlss_that_stopped_working_says_why_and_fsr_still_follows() {
        let mut app = rows_app();
        let u = row(&app, UPSCALER_KEY);
        let want = UpscalerChoice::Dlss(Quality);
        app.world_mut().resource_mut::<Graphics>().upscaler = want;
        let mut status = UpscalerStatus::new(READY);
        status.update(want);
        status.session.dlss = DlssState::Unavailable(Unavailable::EvaluateFailed);
        status.update(want);
        app.insert_resource(status);
        assert_eq!(app.world().resource::<Settings>().choice_unavailable(app.world(), u, "DLSS").as_deref(), Some(Unavailable::EvaluateFailed.words()));
        assert_eq!(chosen(&app), ("DLSS".to_string(), "Quality".to_string()), "the request is kept");
        assert_eq!(app.world().resource::<UpscalerStatus>().effective(), UpscalerChoice::Off, "native, not FSR");
        assert!(nvf_play::settings::pick(app.world_mut(), u, "FSR 3.1"));
        app.world_mut().resource_mut::<UpscalerStatus>().update(UpscalerChoice::Fsr(Quality));
        assert_eq!(app.world().resource::<UpscalerStatus>().effective(), UpscalerChoice::Fsr(Quality), "FSR at once");
    }

    #[test]
    fn with_streamline_every_switch_is_live_and_without_it_dlss_waits_for_a_restart() {
        let mut app = rows_app();
        let (u, q) = (row(&app, UPSCALER_KEY), row(&app, QUALITY_KEY));
        // What follow_request does each frame, without the process-wide loader state.
        let pick = |app: &mut App, i: usize, name: &str| {
            assert!(nvf_play::settings::pick(app.world_mut(), i, name), "{name}");
            let requested = requested(app);
            app.world_mut().resource_mut::<UpscalerStatus>().update(requested);
            let status = *app.world().resource::<UpscalerStatus>();
            (status.effective(), status.restart_pending())
        };
        app.insert_resource(UpscalerStatus::new(READY));
        assert_eq!(pick(&mut app, u, "FSR 3.1"), (UpscalerChoice::Fsr(Quality), false));
        assert_eq!(pick(&mut app, u, "DLSS"), (UpscalerChoice::Dlss(Quality), false), "entering DLSS at once");
        assert_eq!(pick(&mut app, q, "Ultra Performance"), (UpscalerChoice::Dlss(UltraPerformance), false));
        assert_eq!(pick(&mut app, u, "FSR 3.1"), (UpscalerChoice::Fsr(UltraPerformance), false), "leaving DLSS at once");
        assert_eq!(pick(&mut app, u, "Off"), (UpscalerChoice::Off, false));
        assert_eq!(pick(&mut app, u, "DLSS"), (UpscalerChoice::Dlss(UltraPerformance), false));
        // A session that started without Streamline (the files put there since).
        app.insert_resource(UpscalerStatus::new(STOCK));
        assert_eq!(pick(&mut app, u, "FSR 3.1"), (UpscalerChoice::Fsr(UltraPerformance), false), "FSR at once");
        assert_eq!(pick(&mut app, u, "DLSS"), (UpscalerChoice::Fsr(UltraPerformance), true), "DLSS saved, FSR keeps drawing");
        assert_eq!(chosen(&app).0, "DLSS", "the request is what the INI keeps");
        assert_eq!(pick(&mut app, q, "Balanced"), (UpscalerChoice::Fsr(UltraPerformance), true), "DLSS's quality waits with it");
        assert_eq!(pick(&mut app, u, "Off"), (UpscalerChoice::Off, false), "off at once");
    }

    /// An INI written before the two rows has only `fsr-upscaling`: every one of its eleven
    /// names and the early slider build's numbers load into both rows. With the rows' own keys
    /// there, those win.
    #[test]
    fn an_older_ini_loads_into_both_rows() {
        let dir = std::env::temp_dir().join(format!("nvf-upscaler-older-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("NVForever.ini");
        let mut cases: Vec<(String, UpscalerChoice)> = graphics::FSR_UPSCALING_CHOICES.iter().map(|(n, c)| (n.to_string(), *c)).collect();
        cases.extend([("0".to_string(), UpscalerChoice::Off), ("0.2".to_string(), UpscalerChoice::Fsr(NativeAa)), ("0.40".to_string(), UpscalerChoice::Fsr(Quality)), ("1".to_string(), UpscalerChoice::Fsr(UltraPerformance))]);
        cases.push(("dlss balanced".to_string(), UpscalerChoice::Dlss(Balanced)));
        for (text, want) in cases {
            let ini = format!("[Settings]\nfsr-upscaling={text}\n");
            let app = ini_app(&path, &ini, UpscalerChoice::Off, GivenSwitches::default());
            assert_eq!(requested(&app), want, "{text}");
            assert_eq!(upscaler_after_ini(UpscalerChoice::Off, Some(&UpscalerIni::read(&ini)), Some(&GivenSwitches::default())), want, "{text}: the start agrees");
            let (u, q) = chosen(&app);
            assert_eq!(u, UPSCALERS.iter().find(|(_, k)| *k == want.upscaler()).unwrap().0, "{text}");
            assert_eq!(q, want.quality().unwrap_or_default().name(), "{text}");
        }
        let both = "[Settings]\nfsr-upscaling=DLSS Balanced\nupscaler=FSR 3.1\nupscaler-quality=Performance\n";
        assert_eq!(requested(&ini_app(&path, both, UpscalerChoice::Off, GivenSwitches::default())), UpscalerChoice::Fsr(Performance), "the rows' own keys win");
        assert_eq!(UpscalerIni::read(both), UpscalerIni { upscaler: Some("FSR 3.1".into()), quality: Some("Performance".into()) });
        assert_eq!(UpscalerIni::read("[Settings]\nfsr-upscaling=Performance\n"), UpscalerIni { upscaler: Some("FSR 3.1".into()), quality: Some("Performance".into()) }, "split into the two rows' values");
        assert_eq!(UpscalerIni::read("[Settings]\nfsr-upscaling=Off\nupscaler-quality=Balanced\n"), UpscalerIni { upscaler: Some("Off".into()), quality: Some("Balanced".into()) }, "the quality row's own key kept");
        assert_eq!(UpscalerIni::read("[Settings]\nfsr-upscaling=nonsense\n"), UpscalerIni { upscaler: Some("nonsense".into()), quality: None }, "not one of its values: as it is");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The review's P2 (2026-11-12): an INI with only the older `fsr-upscaling=Performance`, a
    /// test run whose `--fsr quality` keeps both rows at the command line's FSR Quality, then a
    /// change of another row: the save writes the stored FSR Performance for both rows, not
    /// FSR with the command line's Quality, and the next start loads FSR Performance.
    #[test]
    fn a_test_run_keeps_the_older_inis_quality_through_a_save() {
        let dir = std::env::temp_dir().join(format!("nvf-upscaler-kept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("NVForever.ini");
        let test_run = GivenSwitches { given: vec!["--fsr".into()], win: true };
        let mut app = ini_app(&path, "[Settings]\nfsr-upscaling=Performance\n", UpscalerChoice::Fsr(Quality), test_run);
        assert_eq!(requested(&app), UpscalerChoice::Fsr(Quality), "the test run's command line");
        let bloom = row(&app, "bloom");
        nvf_play::settings::flip(app.world_mut(), bloom);
        let ini = std::fs::read_to_string(&path).unwrap();
        let get = |key: &str| fnv_install::ini_get(&ini, nvf_play::settings::SECTION, key);
        assert_eq!((get(UPSCALER_KEY), get(QUALITY_KEY)), (Some("FSR 3.1".to_string()), Some("Performance".to_string())), "both rows as the INI had them: {ini}");
        let reloaded = ini_app(&path, &ini, UpscalerChoice::Off, GivenSwitches::default());
        assert_eq!(requested(&reloaded), UpscalerChoice::Fsr(Performance), "the next normal start: FSR Performance");
        assert_eq!(upscaler_after_ini(UpscalerChoice::Off, Some(&UpscalerIni::read(&ini)), Some(&GivenSwitches::default())), UpscalerChoice::Fsr(Performance), "the start agrees");
        // DLSS through the older key alike.
        let test_run = GivenSwitches { given: vec!["--upscaler".into()], win: true };
        let mut app = ini_app(&path, "[Settings]\nfsr-upscaling=DLSS Ultra Performance\n", UpscalerChoice::Fsr(Quality), test_run);
        nvf_play::settings::flip(app.world_mut(), bloom);
        let ini = std::fs::read_to_string(&path).unwrap();
        assert_eq!(requested(&ini_app(&path, &ini, UpscalerChoice::Off, GivenSwitches::default())), UpscalerChoice::Dlss(UltraPerformance), "{ini}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The test aid's `fsr-upscaling=<name>` (the scenarios written for the one row) sets both
    /// rows, logged under the older key.
    #[test]
    fn the_older_key_is_still_taken_by_the_test_aid() {
        let mut app = rows_app();
        app.insert_resource(UpscalerStatus::new(READY));
        let alias = app.world().resource::<Settings>().alias(LEGACY_KEY).map(|(_, a)| a.rows);
        assert_eq!(alias, Some(&[UPSCALER_KEY, QUALITY_KEY][..]), "shown on the Upscaler row");
        for (name, want) in graphics::FSR_UPSCALING_CHOICES {
            let taken = nvf_play::settings::pick_alias(app.world_mut(), LEGACY_KEY, name);
            assert_eq!(taken, requested(&app) == want && name != "Off", "{name}");
            assert_eq!(requested(&app), want, "{name}");
            assert_eq!(legacy_chosen(app.world()), name, "the log line's name");
        }
        assert!(nvf_play::settings::pick_alias(app.world_mut(), LEGACY_KEY, "quality"), "any case");
        assert_eq!(requested(&app), UpscalerChoice::Fsr(Quality));
        assert!(!nvf_play::settings::pick_alias(app.world_mut(), LEGACY_KEY, "0.40"), "names only");
    }

    /// The DLSS model row: right under the quality, NVIDIA's names, every choice round-trips the
    /// INI's text, an unknown stored value loads as Default, and its note names the switch.
    #[test]
    fn the_dlss_model_row_round_trips_and_an_unknown_value_loads_as_default() {
        let mut app = rows_app();
        app.insert_resource(UpscalerStatus::new(READY));
        app.world_mut().resource_mut::<Graphics>().upscaler = UpscalerChoice::Dlss(Quality);
        app.world_mut().resource_mut::<UpscalerStatus>().update(UpscalerChoice::Dlss(Quality));
        let (q, m) = (row(&app, QUALITY_KEY), row(&app, MODEL_KEY));
        let settings = app.world().resource::<Settings>();
        assert_eq!(m, q + 1, "right under Upscaling mode");
        assert_eq!(settings.list()[m].label, "DLSS model");
        let models = settings.choices(m).expect("a dropdown");
        assert_eq!((models.names)(app.world()), ["Default", "K", "L", "M", "J"]);
        assert_eq!(model_chosen(app.world()), "Default", "Default by default");
        for model in DlssModel::ALL {
            nvf_play::settings::pick(app.world_mut(), m, model.name());
            assert_eq!(app.world().resource::<Graphics>().dlss_model, model);
            let saved = (models.save)(app.world());
            assert_eq!(saved, model.name());
            app.world_mut().resource_mut::<Graphics>().dlss_model = DlssModel::Default;
            assert!((models.load)(app.world_mut(), &saved));
            assert_eq!(app.world().resource::<Graphics>().dlss_model, model, "{saved}");
        }
        assert!(pick_model(app.world_mut(), "k"), "any case");
        assert_eq!(app.world().resource::<Graphics>().dlss_model, DlssModel::K);
        assert!(!pick_model(app.world_mut(), "A"), "not one of the five: not taken");
        assert_eq!(app.world().resource::<Graphics>().dlss_model, DlssModel::K);
        // The INI's earlier spellings (`Preset K` …) load into the same models.
        for (older, model) in [("Preset J", DlssModel::J), ("preset k", DlssModel::K), ("Preset L", DlssModel::L), ("PRESET M", DlssModel::M)] {
            app.world_mut().resource_mut::<Graphics>().dlss_model = DlssModel::Default;
            assert!(load_model(app.world_mut(), older), "{older:?}");
            assert_eq!(app.world().resource::<Graphics>().dlss_model, model, "{older:?}");
            assert_eq!(model_chosen(app.world()), model.name(), "saved by its letter from then on");
        }
        for unknown in ["A", "Preset A", "N", "Preset N", "Preset Default", "", "13", "transformer"] {
            app.world_mut().resource_mut::<Graphics>().dlss_model = DlssModel::L;
            assert!(load_model(app.world_mut(), unknown), "{unknown:?}");
            assert_eq!(app.world().resource::<Graphics>().dlss_model, DlssModel::Default, "{unknown:?} loads as Default");
        }
        assert_eq!(nvf_play::settings::note_switches(MODEL_NOTE), ["--dlss-model"], "a test run's --dlss-model keeps the row at the command line's model");
        let label = |name: &str| app.world().resource::<Settings>().choice_label(app.world(), m, name);
        assert_eq!(label("L"), "L", "NVIDIA's letters");
        assert_eq!(label("Default"), "Default", "in English here (the page translates it)");
    }

    /// The DLSS model row stands dimmed with a reason while DLSS is not the effective upscaler
    /// (FSR, off, DLSS waiting for a restart or given up on); the model is kept meanwhile and
    /// the INI still loads into it.
    #[test]
    fn the_dlss_model_row_is_dimmed_while_dlss_does_not_draw() {
        let mut app = rows_app();
        let m = row(&app, MODEL_KEY);
        let dimmed = |app: &App| app.world().resource::<Settings>().disabled_status(app.world(), m);
        let ask = |app: &mut App, choice: UpscalerChoice| {
            app.world_mut().resource_mut::<Graphics>().upscaler = choice;
            app.world_mut().resource_mut::<UpscalerStatus>().update(choice);
        };
        app.insert_resource(UpscalerStatus::new(READY));
        ask(&mut app, UpscalerChoice::Off);
        assert_eq!(dimmed(&app), Some("DLSS only"));
        assert!(!nvf_play::settings::pick(app.world_mut(), m, "L"), "dimmed: not taken from the page or a test aid");
        ask(&mut app, UpscalerChoice::Fsr(Quality));
        assert_eq!(dimmed(&app), Some("DLSS only"), "FSR draws");
        ask(&mut app, UpscalerChoice::Dlss(Balanced));
        assert_eq!(dimmed(&app), None, "DLSS draws");
        assert!(nvf_play::settings::pick(app.world_mut(), m, "L"));
        ask(&mut app, UpscalerChoice::Fsr(Balanced));
        assert_eq!(dimmed(&app), Some("DLSS only"));
        assert_eq!(model_chosen(app.world()), "L", "kept while FSR draws");
        assert!(load_model(app.world_mut(), "M"), "the INI loads into it all the same");
        assert_eq!(app.world().resource::<Graphics>().dlss_model, DlssModel::M);
        // DLSS asked for but waiting for a restart (FSR drawing until then), or given up on.
        let mut waiting = UpscalerStatus::new(STOCK);
        waiting.update(UpscalerChoice::Fsr(Quality));
        app.insert_resource(waiting);
        ask(&mut app, UpscalerChoice::Dlss(Quality));
        assert!(app.world().resource::<UpscalerStatus>().restart_pending());
        assert_eq!(dimmed(&app), Some("DLSS only"), "DLSS asked for, not drawing yet");
        app.insert_resource(UpscalerStatus::new(Session { streamline_loaded: true, dlss: DlssState::Unavailable(Unavailable::EvaluateFailed), fsr: None }));
        ask(&mut app, UpscalerChoice::Dlss(Quality));
        assert_eq!(dimmed(&app), Some("DLSS only"), "DLSS given up on: native draws");
    }

    /// The INI's `dlss-model` line by the settings' precedence: applied in normal play; in a test
    /// run a given `--dlss-model` keeps the command line's model.
    #[test]
    fn the_dlss_model_follows_the_settings_precedence() {
        let dir = std::env::temp_dir().join(format!("nvf-dlss-model-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("NVForever.ini");
        let model = |app: &App| app.world().resource::<Graphics>().dlss_model;
        let given = |switches: &[&str], win: bool| GivenSwitches { given: switches.iter().map(|s| s.to_string()).collect(), win };
        let app = ini_app(&path, "[Settings]
dlss-model=J
", UpscalerChoice::Off, GivenSwitches::default());
        assert_eq!(model(&app), DlssModel::J, "the INI's model");
        let app = ini_app(&path, "[Settings]
dlss-model=J
", UpscalerChoice::Off, given(&["--dlss-model"], false));
        assert_eq!(model(&app), DlssModel::J, "normal play: the INI wins over --dlss-model");
        let app = ini_app(&path, "[Settings]
dlss-model=J
", UpscalerChoice::Off, given(&["--dlss-model"], true));
        assert_eq!(model(&app), DlssModel::Default, "a test run: --dlss-model (the command line's Default here) is kept");
        let app = ini_app(&path, "[Settings]
dlss-model=J
", UpscalerChoice::Off, given(&["--upscaler"], true));
        assert_eq!(model(&app), DlssModel::J, "another switch given keeps nothing of this row");
        let app = ini_app(&path, "[Settings]
dlss-model=Preset Z
", UpscalerChoice::Off, GivenSwitches::default());
        assert_eq!(model(&app), DlssModel::Default, "an unknown stored value loads as Default");
        let app = ini_app(&path, "[Settings]
dlss-model=Preset K
", UpscalerChoice::Off, GivenSwitches::default());
        assert_eq!(model(&app), DlssModel::K, "the earlier spelling loads");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn both_notes_name_the_same_switches() {
        let switches = |note| {
            let mut s = nvf_play::settings::note_switches(note);
            s.sort();
            s
        };
        assert_eq!(switches(UPSCALER_NOTE), ["--fsr", "--upscaler", "--upscaler-quality"]);
        assert_eq!(switches(QUALITY_NOTE), switches(UPSCALER_NOTE), "a switch given keeps both rows or neither");
    }

    /// A settings page with the INI at `path` holding `text`, `apply_stored` run once.
    fn ini_app(path: &std::path::Path, text: &str, command_line: UpscalerChoice, given: GivenSwitches) -> App {
        ini_app_with_frame_generation(path, text, command_line, given, crate::frame_gen::FrameGenKind::Off)
    }

    fn ini_app_with_frame_generation(path: &std::path::Path, text: &str, command_line: UpscalerChoice, given: GivenSwitches, frame_generation: crate::frame_gen::FrameGenKind) -> App {
        std::fs::write(path, text).unwrap();
        let mut app = App::new();
        let mut graphics = Graphics::preset(graphics::Preset::Vanilla);
        graphics.upscaler = command_line;
        app.insert_resource(graphics)
            .insert_resource(graphics::MsaaSetting(Msaa::Off))
            .insert_resource(crate::frame_gen::FrameGeneration { kind: frame_generation })
            .init_resource::<crate::frame_gen::DlssgFrames>()
            .insert_resource(crate::anti_lag::LatencyReduction { on: false, by_frame_generation: false })
            .insert_resource(SettingsFile(Some(path.to_path_buf())))
            .insert_resource(given);
        register_graphics(&mut app);
        app.add_systems(PostStartup, apply_stored);
        app.update();
        app
    }

    #[test]
    fn saved_frame_generation_is_changed_and_defaults_restore_the_command_line() {
        use crate::frame_gen::{FrameGenKind as K, FrameGeneration};
        use nvf_play::settings::{changed_rows, restore, StartChoices, StartStates};
        let dir = std::env::temp_dir().join(format!("nvf-frame-generation-defaults-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("NVForever.ini");
        for (saved, command_line, win, want) in [
            ("true", K::Off, false, K::Fsr),
            ("fsr", K::Off, false, K::Fsr),
            ("dlss", K::Off, false, K::Dlss),
            ("dlss", K::Fsr, false, K::Dlss),
            ("dlss", K::Fsr, true, K::Fsr),
            ("true", K::Off, true, K::Off),
        ] {
            let text = format!("[Settings]\nframe-generation={saved}\n");
            let given = GivenSwitches { given: if command_line != K::Off || win { ["--frame-generation".into()].into() } else { Default::default() }, win };
            // Only the loader gets the early INI-resolved choice (main.rs).
            assert_eq!(frame_generation_after_ini(command_line, Some(&text), Some(&given)), want);
            let mut app = ini_app_with_frame_generation(&path, &text, UpscalerChoice::Off, given, command_line);
            let world = app.world_mut();
            let row = world.resource::<Settings>().list().iter().position(|s| s.key == FRAME_GENERATION_KEY).unwrap();
            let baseline = world.resource::<StartStates>().0.clone();
            assert_eq!(world.resource::<FrameGeneration>().kind, want, "INI/command-line precedence");
            assert_eq!(world.resource::<StartChoices>().0.iter().find(|(i, _)| *i == row).unwrap().1, command_line.stored());
            assert_eq!(changed_rows(world, &baseline).contains(&row), want != command_line, "Changed compares with the pre-INI choice");
            restore(world, row, baseline[row]);
            assert_eq!(world.resource::<FrameGeneration>().kind, command_line, "Defaults restores the pre-INI choice");
            assert!(!changed_rows(world, &baseline).contains(&row));
        }
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    /// The Frame generation row's names, INI texts and the old switch's values; the Generated
    /// frames row's choices (#2317).
    #[test]
    fn frame_generation_choices_read_names_texts_and_the_old_switch() {
        use crate::frame_gen::FrameGenKind as K;
        for (text, kind) in [("Off", K::Off), ("FSR 3.1", K::Fsr), ("DLSS", K::Dlss), ("dlss", K::Dlss), ("fsr", K::Fsr), ("true", K::Fsr), ("false", K::Off), ("on", K::Fsr), ("off", K::Off)] {
            assert_eq!(K::parse(text), Some(kind), "{text}");
        }
        assert_eq!(K::parse("maybe"), None);
        assert_eq!(K::ALL.map(K::stored), ["off", "fsr", "dlss"]);
        assert_eq!((frames_named("2x"), frames_named("4X"), frames_named("5x")), (Some(1), Some(3), None));
        // The GPU's limit is not known in the tests: no choice is held back.
        assert_eq!(frames_unavailable(&World::new(), "4x"), None);
        let ini = "[Settings]\nframe-generation=dlss\n";
        let given = nvf_play::settings::GivenSwitches { given: Default::default(), win: false };
        assert_eq!(frame_generation_after_ini(K::Off, Some(ini), Some(&given)), K::Dlss, "the INI wins without the switch");
        assert_eq!(frame_generation_after_ini(K::Fsr, Some(ini), None), K::Fsr, "without a settings page the command line decides");
    }

    /// The start's choice (before the renderer, `main`) and `apply_stored`'s at `PostStartup`
    /// agree for every precedence case, for the rows' keys and the older one.
    #[test]
    fn the_start_and_the_settings_read_the_upscaler_alike() {
        let dir = std::env::temp_dir().join(format!("nvf-upscaler-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("NVForever.ini");
        let fsr_q = UpscalerChoice::Fsr(Quality);
        let dlss_b = UpscalerChoice::Dlss(Balanced);
        // The INI's lines, the command line, the switches given, whether they win, the result.
        type Case<'a> = (&'a str, UpscalerChoice, &'a [&'a str], bool, UpscalerChoice);
        let cases: [Case; 18] = [
            ("fsr-upscaling=DLSS Balanced", fsr_q, &["--fsr"], false, dlss_b),
            ("fsr-upscaling=DLSS Balanced", fsr_q, &["--fsr"], true, fsr_q),
            ("fsr-upscaling=DLSS Balanced", fsr_q, &["--upscaler"], true, fsr_q),
            ("fsr-upscaling=DLSS Balanced", fsr_q, &["--upscaler-quality"], true, fsr_q),
            ("fsr-upscaling=DLSS Balanced", UpscalerChoice::Off, &["--msaa"], true, dlss_b),
            ("fsr-upscaling=Quality", dlss_b, &[], false, fsr_q),
            ("fsr-upscaling=0.40", UpscalerChoice::Off, &[], false, fsr_q),
            ("fsr-upscaling=2", dlss_b, &[], false, dlss_b),
            ("fsr-upscaling=nonsense", fsr_q, &[], false, fsr_q),
            ("bloom=0", dlss_b, &[], false, dlss_b),
            ("upscaler=DLSS\nupscaler-quality=Balanced", fsr_q, &[], false, dlss_b),
            ("upscaler=DLSS\nupscaler-quality=Balanced", fsr_q, &["--upscaler"], true, fsr_q),
            ("upscaler=DLSS\nupscaler-quality=Balanced", fsr_q, &["--upscaler-quality"], true, fsr_q),
            ("upscaler=DLSS", UpscalerChoice::Fsr(Performance), &[], false, UpscalerChoice::Dlss(Performance)),
            ("upscaler-quality=Balanced", fsr_q, &[], false, UpscalerChoice::Fsr(Balanced)),
            ("upscaler-quality=Balanced", UpscalerChoice::Off, &[], false, UpscalerChoice::Off),
            ("upscaler=Off\nupscaler-quality=Balanced", dlss_b, &[], false, UpscalerChoice::Off),
            ("fsr-upscaling=Performance\nupscaler=DLSS\nupscaler-quality=Ultra Performance", fsr_q, &[], false, UpscalerChoice::Dlss(UltraPerformance)),
        ];
        for (lines, command_line, given, win, want) in cases {
            let text = format!("[Settings]\n{lines}\n");
            let given = GivenSwitches { given: given.iter().map(|g| g.to_string()).collect(), win };
            let early = upscaler_after_ini(command_line, Some(&UpscalerIni::read(&text)), Some(&given));
            let late = requested(&ini_app(&path, &text, command_line, given.clone()));
            assert_eq!(early, want, "{lines:?} {command_line:?} {given:?}");
            assert_eq!(late, early, "{lines:?} {command_line:?} {given:?}: the start and the settings disagree");
        }
        // No settings page (not --play): the INI is not applied, the command line decides.
        assert_eq!(upscaler_after_ini(fsr_q, Some(&UpscalerIni::read("[Settings]\nupscaler=DLSS\n")), None), fsr_q);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
