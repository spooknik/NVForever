//! Whether the landscape is smoothed (#2149, `fnv_world::land_surface`): one switch for
//! the whole run, so the drawn ground (nvforever terrain.rs), its collision
//! (`crate::phys`) and every height query stand on the same surface.
//!
//! [design, #2149] On by default (maintainer decision 2026-11-13: "terrain smoothing should
//! be the default have the collision follow the smoothed surface"); `--no-smooth-terrain`
//! or the settings page's Smooth terrain row turns it off. A change on the page takes
//! effect at the next start (loading a save starts the game again): cells are drawn and
//! collided as they load, so a change while the game runs would leave the loaded cells'
//! ground and collision on the old surface beside new cells on the other [design].

use std::sync::atomic::{AtomicBool, Ordering};

/// What the settings page shows and the INI keeps (the next start's state).
static SETTING: AtomicBool = AtomicBool::new(true);
/// This run's state, fixed once the INI is read ([`fix`]).
static RUN: AtomicBool = AtomicBool::new(true);

/// The INI key and settings row.
pub const KEY: &str = "smooth-terrain";

/// The settings row's note, NVForever's own words [design].
pub const NOTE: &str = "On: the landscape between its points is drawn and walked on as a smooth surface, each grid square in four, never more than one height step from the stored ground, so slopes and ridges lose their steps and zig-zags; actors and objects stand on what is drawn. Off: the landscape's triangles as stored. Takes effect at the next start (loading a save starts the game again). Not in the original. (--no-smooth-terrain)";

/// The state the settings page shows (what the next start uses).
pub fn setting() -> bool {
    SETTING.load(Ordering::Relaxed)
}

pub fn set_setting(on: bool) {
    SETTING.store(on, Ordering::Relaxed);
}

/// Whether this run's landscape is smoothed.
pub fn on() -> bool {
    RUN.load(Ordering::Relaxed)
}

/// Fix the run's state from the setting: before the first cell loads (once at start from
/// the command line, again at `PostStartup` after the INI is applied).
pub fn fix() {
    RUN.store(setting(), Ordering::Relaxed);
}

/// The start's log line (the smoke scenario `terrain-smoothing` reads it).
pub fn describe() -> String {
    if on() {
        format!(
            "terrain: smoothed, each grid square drawn and collided as {0} x {0}, at most {1} units from the stored triangles",
            fnv_world::land_surface::SUBDIV,
            fnv_world::land_surface::MAX_DEVIATION
        )
    } else {
        "terrain: the stored triangles, not smoothed (--no-smooth-terrain or the Smooth terrain setting)".to_string()
    }
}
