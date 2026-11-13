//! The mouse pointer while the controller works the controls [design, #2310].
//!
//! With Input Device = Controller the window's pointer **sleeps**: it is hidden in the
//! menus (`crate::player::grab_cursor`) and points at nothing, so no row, button or tab
//! under the spot where the mouse was left is hovered or takes a selection
//! (`crate::ui_pointer`, the menus drawn from their files in `crate::tiles::input`, the
//! Pip-Boy's screen in `crate::pipboy::flat_pointer` and the arm's in nvforever's
//! `pipboy_arm`). It wakes when the mouse is used: moved [`WAKE_TRAVEL`] pixels, a button
//! or the wheel; then the pointer shows and hovers as with the keyboard and mouse. It
//! sleeps again on the controller's next button press or stick push, and whenever the
//! device turns to Controller (the Controls tab, or Automatic Input Switching on a button,
//! `super::device`). Mouse motion does not switch the device (`super::device`): it only
//! wakes the pointer, so a player who reaches for the mouse in a menu gets it back
//! without the controller's prompts going away.
//!
//! [design: the issue's report, "the cursor ... tends to pop up and stick a lot"; whether
//! the original hides its pointer with a controller is a hypothesis not checked, #2310.]
//! Test aids that put their own pointer somewhere (`NVF_PLAY_TEST_PIPBOY`'s `point=`,
//! `NVF_PLAY_TEST_POINTER`) stand in for the mouse and are not put to sleep.

use crate::bindings::{Bindings, InputDevice};
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::CursorMoved;

/// How far (logical pixels, summed over frames) the mouse must travel to wake a sleeping
/// pointer: a desk's jolt or a sensor's jitter does not [design: as the wheel's hold,
/// `crate::menu::WHEEL_HOLD_TRAVEL`].
pub const WAKE_TRAVEL: f32 = 8.0;

/// Whether the window's pointer sleeps (module notes).
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct PadPointer {
    asleep: bool,
    /// The mouse's travel since the pointer fell asleep.
    travel: f32,
}

impl PadPointer {
    /// The pointer is hidden and points at nothing.
    pub fn asleep(&self) -> bool {
        self.asleep
    }
}

/// Whether `pointer` (a [`PadPointer`] or none, for tools and tests without the controller
/// plugin) sleeps.
pub fn asleep(pointer: Option<&PadPointer>) -> bool {
    pointer.is_some_and(PadPointer::asleep)
}

/// One frame's input, as [`step`] reads it.
#[derive(Debug, Default, Clone, Copy)]
pub struct Frame {
    /// The device is Controller.
    pub controller: bool,
    /// It turned to Controller this frame (or is Controller on the first frame).
    pub became_controller: bool,
    /// A controller button went down or a stick crossed its dead zone.
    pub pad: bool,
    /// The mouse's travel this frame (logical pixels).
    pub travel: f32,
    /// A mouse button went down or the wheel turned.
    pub mouse_used: bool,
}

/// The pointer after a frame (module notes): awake with the keyboard and mouse; asleep as
/// the device turns to Controller and on the controller's use (which wins over the mouse in
/// the same frame); awake again on a mouse button, the wheel or [`WAKE_TRAVEL`] of motion.
pub fn step(p: PadPointer, f: Frame) -> PadPointer {
    if !f.controller {
        return PadPointer::default();
    }
    if f.became_controller || f.pad {
        return PadPointer { asleep: true, travel: 0.0 };
    }
    if !p.asleep {
        return p;
    }
    let travel = p.travel + f.travel;
    if f.mouse_used || travel >= WAKE_TRAVEL {
        PadPointer::default()
    } else {
        PadPointer { asleep: true, travel }
    }
}

/// Keep [`PadPointer`] from the device and this frame's mouse and controller input; runs
/// after the device is chosen (`super::device::automatic`) and before anything reads the
/// pointer (UI focus, picking, the Pip-Boy's pointer). Logs each change.
#[allow(clippy::too_many_arguments)]
pub fn follow(
    bindings: Option<Res<Bindings>>,
    pads: Query<&Gamepad>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut moved: MessageReader<CursorMoved>,
    mut wheel: MessageReader<MouseWheel>,
    mut pointer: ResMut<PadPointer>,
    mut before: Local<Option<InputDevice>>,
    mut pushed: Local<bool>,
) {
    let device = bindings.map_or(InputDevice::KeyboardMouse, |b| b.device);
    let controller = device == InputDevice::Controller;
    let became_controller = controller && *before != Some(InputDevice::Controller);
    *before = Some(device);
    let push = pads.iter().any(|g| g.left_stick().length() > super::STICK_DEAD || g.right_stick().length() > super::STICK_DEAD);
    let pad = pads.iter().any(|g| g.get_just_pressed().next().is_some()) || (push && !*pushed);
    *pushed = push;
    let travel: f32 = moved.read().map(|m| m.delta.map_or(0.0, |d| d.length())).sum();
    let wheeled = wheel.read().count() > 0;
    let mouse_used = wheeled || mouse.get_just_pressed().next().is_some();
    let next = step(*pointer, Frame { controller, became_controller, pad, travel, mouse_used });
    if next.asleep != pointer.asleep {
        info!("pointer: {}", if next.asleep { "asleep (the controller has the controls)" } else { "awake (the mouse was used)" });
    }
    if next != *pointer {
        *pointer = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad_frame() -> Frame {
        Frame { controller: true, pad: true, ..Frame::default() }
    }

    #[test]
    fn the_keyboard_and_mouse_never_put_it_to_sleep() {
        let p = step(PadPointer { asleep: true, travel: 3.0 }, Frame { pad: true, became_controller: true, ..Frame::default() });
        assert_eq!(p, PadPointer::default(), "Keyboard and Mouse: always awake");
    }

    #[test]
    fn it_sleeps_as_the_device_turns_to_controller_and_on_the_controllers_use() {
        let p = step(PadPointer::default(), Frame { controller: true, became_controller: true, ..Frame::default() });
        assert!(p.asleep());
        let awake = step(p, Frame { controller: true, mouse_used: true, ..Frame::default() });
        assert!(!awake.asleep());
        assert!(step(awake, pad_frame()).asleep(), "a button press puts it back to sleep");
        assert!(!step(awake, Frame { controller: true, ..Frame::default() }).asleep(), "a quiet frame changes nothing");
    }

    #[test]
    fn motion_wakes_it_only_once_it_adds_up_to_the_travel() {
        let mut p = step(PadPointer::default(), Frame { controller: true, became_controller: true, ..Frame::default() });
        for _ in 0..3 {
            p = step(p, Frame { controller: true, travel: 2.0, ..Frame::default() });
            assert!(p.asleep(), "jitter keeps it asleep");
        }
        p = step(p, Frame { controller: true, travel: 2.0, ..Frame::default() });
        assert!(!p.asleep(), "8 pixels in all wake it");
    }

    #[test]
    fn the_controller_wins_a_frame_both_are_used_in() {
        let p = step(PadPointer::default(), Frame { controller: true, pad: true, mouse_used: true, travel: 50.0, ..Frame::default() });
        assert!(p.asleep());
    }

    #[test]
    fn the_system_follows_the_device_the_mouse_and_the_pad() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::input::InputPlugin));
        app.add_message::<CursorMoved>();
        app.init_resource::<Bindings>().init_resource::<PadPointer>().add_systems(Update, follow);
        app.update();
        assert!(!app.world().resource::<PadPointer>().asleep(), "Keyboard and Mouse");
        app.world_mut().resource_mut::<Bindings>().device = InputDevice::Controller;
        app.update();
        assert!(app.world().resource::<PadPointer>().asleep(), "Controller chosen");
        app.world_mut().write_message(CursorMoved { window: Entity::PLACEHOLDER, position: Vec2::new(20.0, 0.0), delta: Some(Vec2::new(20.0, 0.0)) });
        app.update();
        assert!(!app.world().resource::<PadPointer>().asleep(), "the mouse moved");
        app.world_mut().spawn(Gamepad::default());
        app.update();
        let pad = app.world_mut().query_filtered::<Entity, With<Gamepad>>().single(app.world()).unwrap();
        use bevy::input::gamepad::{RawGamepadButtonChangedEvent, RawGamepadEvent};
        app.world_mut().write_message(RawGamepadEvent::Button(RawGamepadButtonChangedEvent::new(pad, GamepadButton::DPadDown, 1.0)));
        app.update();
        assert!(app.world().resource::<PadPointer>().asleep(), "the D-pad pressed");
    }
}
