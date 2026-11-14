//! Hardcore mode as a switch of the settings page's Gameplay group (kept in
//! `NVForever.ini` as every row is) [owner 2026-10-25]. The rules are `fnv_game::needs`'s
//! (`Game::set_hardcore`: turning it off clears the needs and their stages' effects);
//! this keeps the switch and the running game agreeing each frame:
//!
//! - A flip of the switch is handed to the game (SetHardcore).
//! - The game changing it itself (a script's or the console's SetHardcore, a loaded save
//!   that has it set) moves the switch, so the page shows what the game plays by.
//! - A new game that never set it plays by the switch.

use crate::script::{with_world, ScriptState};
use crate::settings::{add_setting, Setting};
use crate::sim::SimState;
use bevy::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

/// The switch, for the process: the settings page (and the INI) set it.
static ON: AtomicBool = AtomicBool::new(false);

/// Whether the switch is on.
pub fn on() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Turn the switch on or off; the game takes it on its next frame.
pub fn set_on(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

pub struct HardcorePlugin;

impl Plugin for HardcorePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync.in_set(crate::PlaySet::Input));
        add_setting(
            app,
            Setting {
                key: "hardcore",
                group: crate::pause::GROUP_MENUS,
                label: "Hardcore mode",
                note: "On: you need water, food and sleep, stimpaks heal over time, ammo has weight, crippled limbs need a doctor's bag or a doctor. Off: none of that.",
                switches: "",
                get: |_| on(),
                set: |_, v| set_on(v),
                slider: None,
            },
        );
    }
}

/// What to do this frame: the game's own setting (`None`: never set), what it plays by,
/// the switch, and what the switch and the game last agreed on.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    /// They agree.
    Keep,
    /// The game set it itself: the switch follows.
    Follow(bool),
    /// The switch was flipped (or a new game never set it): hand it to the game.
    Hand(bool),
}

fn step(set: Option<bool>, plays: bool, switch: bool, agreed: Option<bool>) -> Step {
    match set {
        Some(g) if Some(g) != agreed && g != switch => Step::Follow(g),
        _ if plays == switch => Step::Keep,
        _ => Step::Hand(switch),
    }
}

/// Keep the switch and the running game agreeing (module notes), and log a change.
fn sync(mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, mut agreed: Local<Option<bool>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let switch = on();
    let (set, plays) = with_world(rt, &mut sim, |rt| (rt.game.player.hardcore, rt.game.hardcore()));
    match step(set, plays, switch, *agreed) {
        Step::Keep => *agreed = Some(switch),
        Step::Follow(g) => {
            set_on(g);
            *agreed = Some(g);
            info!("hardcore: the game turned Hardcore mode {}; the setting follows", if g { "on" } else { "off" });
        }
        Step::Hand(v) => {
            with_world(rt, &mut sim, |rt| rt.game.set_hardcore(v));
            *agreed = Some(v);
            info!("hardcore: Hardcore mode {}{}", if v { "on" } else { "off" }, if v { "" } else { ": needs cleared" });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_and_the_game_follow_each_other() {
        // A new game that never set it, the switch off: nothing to do (no SetHardcore 0,
        // which would mark it as not always Hardcore).
        assert_eq!(step(None, false, false, None), Step::Keep);
        // The switch on for a new game: handed to it.
        assert_eq!(step(None, false, true, None), Step::Hand(true));
        // The switch flipped off after they agreed on on.
        assert_eq!(step(Some(true), true, false, Some(true)), Step::Hand(false));
        // A script's SetHardcore 1 with the switch off: the switch follows.
        assert_eq!(step(Some(true), true, false, Some(false)), Step::Follow(true));
        // A loaded save with Hardcore on, first frame: the save wins.
        assert_eq!(step(Some(true), true, false, None), Step::Follow(true));
        // Agreed.
        assert_eq!(step(Some(true), true, true, Some(true)), Step::Keep);
        // A new game (never set) after one played with the switch on: handed again.
        assert_eq!(step(None, false, true, Some(true)), Step::Hand(true));
    }
}
