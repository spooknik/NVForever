//! The flashlight (owner request 2026-10-11): holding Menu Mode (Tab) switches a light
//! on the camera on or off; a tap still opens the inventory, when the key is let go.
//! [design: the original's hold of the same key works its Pip-Boy light, a glow round the
//! player; this one is a cone ahead of the view, as other games' flashlights are. Whether
//! the original toggles at a set hold time is not captured.]
//!
//! - The key counts as held from a press made while playing; a hold of [`HOLD_SECONDS`]
//!   toggles the light at once (the key still down), and letting go after it opens
//!   nothing. A key that was down when a menu closed (Tab closes them) is not a press.
//! - A press another system has used already ([`MenuKeyHold::consume`]) opens nothing
//!   and toggles nothing: a Menu Mode press made in VATS mode (B on a controller, Tab on
//!   the keyboard) only leaves it, or on the controller's classic screen returns
//!   (`crate::vats_mode::read_keys`) [design, #2311: B is a controller's back button;
//!   leaving and opening the Pip-Boy on its release was reported as a bug].
//! - The light is not held by a script's `DisablePlayerControls` (the Pip-Boy flag holds
//!   only the inventory, `menu::open_menus`) [design].
//! - nvforever draws it (`flashlight.rs`): a spot light on the player's camera while
//!   [`Flashlight::on`].
//!
//! Test aid: `NVF_PLAY_TEST_CONTROL=<seconds>:flashlight` toggles it as a hold would.

use crate::bindings::{Control, ControlInput};
use crate::menu::OpenInventory;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;

/// How long Menu Mode is held to switch the flashlight [design: within the owner's "1–2
/// seconds", at the quick end].
pub const HOLD_SECONDS: f32 = 1.0;

/// Whether the flashlight is on.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct Flashlight {
    pub on: bool,
    /// Times switched this run.
    pub toggles: u32,
}

impl Flashlight {
    pub fn toggle(&mut self) {
        self.on = !self.on;
        self.toggles += 1;
        info!("flashlight: {}", if self.on { "on" } else { "off" });
    }
}

/// What the Menu Mode key asks for this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOutcome {
    Nothing,
    OpenInventory,
    ToggleLight,
}

/// The Menu Mode key's hold.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct MenuKeyHold {
    /// Seconds held, from a press made while playing.
    held: Option<f32>,
    /// The hold has toggled the light already.
    toggled: bool,
}

impl MenuKeyHold {
    /// One frame of the key: `down` and `pressed_now` as the input says, `dt` seconds since
    /// the last frame, `playing` whether the game is in play (no menu, conversation or
    /// loading screen).
    pub fn step(&mut self, down: bool, pressed_now: bool, dt: f32, playing: bool) -> KeyOutcome {
        if !playing {
            self.held = None;
            return KeyOutcome::Nothing;
        }
        if pressed_now {
            (self.held, self.toggled) = (Some(0.0), false);
        }
        let Some(t) = self.held.as_mut() else { return KeyOutcome::Nothing };
        if down {
            *t += dt;
            if !self.toggled && *t >= HOLD_SECONDS {
                self.toggled = true;
                return KeyOutcome::ToggleLight;
            }
            return KeyOutcome::Nothing;
        }
        self.held = None;
        if self.toggled {
            KeyOutcome::Nothing
        } else {
            KeyOutcome::OpenInventory
        }
    }

    /// Use up the press being held: its release opens nothing and its hold toggles
    /// nothing, until the next press. For a system that acted on the press itself (a
    /// mode the press ended); run after [`PlaySet::Input`], where the press is recorded.
    pub fn consume(&mut self) {
        self.held = None;
    }
}

pub struct FlashlightPlugin;

impl Plugin for FlashlightPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Flashlight>().init_resource::<MenuKeyHold>().add_systems(Update, menu_key.in_set(PlaySet::Input));
    }
}

pub(crate) fn menu_key(
    input: ControlInput,
    time: Res<Time<Real>>,
    state: Res<State<PlayState>>,
    mut hold: ResMut<MenuKeyHold>,
    mut light: ResMut<Flashlight>,
    mut inventory: MessageWriter<OpenInventory>,
) {
    let playing = *state.get() == PlayState::Playing;
    match hold.step(input.pressed(Control::MenuMode), input.just_pressed(Control::MenuMode), time.delta_secs(), playing) {
        KeyOutcome::Nothing => {}
        KeyOutcome::OpenInventory => {
            inventory.write(OpenInventory);
        }
        KeyOutcome::ToggleLight => light.toggle(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run the key down for `secs` in frames of `dt`, then up; every outcome but Nothing.
    fn hold_for(h: &mut MenuKeyHold, secs: f32, dt: f32) -> Vec<KeyOutcome> {
        let mut out = vec![h.step(true, true, dt, true)];
        let mut t = dt;
        while t < secs {
            out.push(h.step(true, false, dt, true));
            t += dt;
        }
        out.push(h.step(false, false, dt, true));
        out.retain(|o| *o != KeyOutcome::Nothing);
        out
    }

    #[test]
    fn a_tap_opens_the_inventory_when_let_go() {
        let mut h = MenuKeyHold::default();
        assert_eq!(h.step(true, true, 1.0 / 60.0, true), KeyOutcome::Nothing);
        assert_eq!(hold_for(&mut h, 0.3, 1.0 / 60.0), vec![KeyOutcome::OpenInventory]);
    }

    #[test]
    fn a_hold_toggles_the_light_once_and_opens_nothing() {
        let mut h = MenuKeyHold::default();
        assert_eq!(hold_for(&mut h, 3.0, 1.0 / 60.0), vec![KeyOutcome::ToggleLight]);
        // Just short of the hold: still a tap.
        assert_eq!(hold_for(&mut h, HOLD_SECONDS - 0.1, 1.0 / 60.0), vec![KeyOutcome::OpenInventory]);
    }

    #[test]
    fn a_consumed_press_opens_nothing_and_toggles_nothing() {
        let dt = 1.0 / 60.0;
        // A tap whose press ended a mode (#2311): let go, nothing opens.
        let mut h = MenuKeyHold::default();
        assert_eq!(h.step(true, true, dt, true), KeyOutcome::Nothing);
        h.consume();
        assert_eq!(h.step(false, false, dt, true), KeyOutcome::Nothing);
        // Held past the light's time: nothing toggles, and its release opens nothing.
        assert_eq!(h.step(true, true, dt, true), KeyOutcome::Nothing);
        h.consume();
        let mut t = 0.0;
        while t < 3.0 {
            assert_eq!(h.step(true, false, dt, true), KeyOutcome::Nothing);
            t += dt;
        }
        assert_eq!(h.step(false, false, dt, true), KeyOutcome::Nothing);
        // The next press is a tap again.
        assert_eq!(hold_for(&mut h, 0.2, dt), vec![KeyOutcome::OpenInventory]);
    }

    #[test]
    fn a_key_down_from_a_menu_is_not_a_press() {
        let mut h = MenuKeyHold::default();
        // Tab closes the menu: the key is down when play resumes, then let go.
        assert_eq!(h.step(true, true, 0.016, false), KeyOutcome::Nothing);
        assert_eq!(h.step(true, false, 0.016, true), KeyOutcome::Nothing);
        assert_eq!(h.step(false, false, 0.016, true), KeyOutcome::Nothing);
        // A press cut off by a conversation starting opens nothing afterwards.
        h.step(true, true, 0.016, true);
        h.step(true, false, 0.016, false);
        assert_eq!(h.step(false, false, 0.016, true), KeyOutcome::Nothing);
    }
}
