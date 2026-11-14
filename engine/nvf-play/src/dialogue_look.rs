//! The player's view in a conversation (dialogue step D7, docs/design/dialogue-play.md
//! §8): while the dialogue screen is open the player's view turns to the face of the
//! actor they talk to. The actor's side (its head and body turning to the player) is the
//! renderer's (`engine/nvforever/src/dialogue_look.rs`), which also says where the face
//! is ([`SpeakerFace`]).
//!
//! Only conversations (`Game::dialogue`: E on an actor, a script's StartConversation).
//! A line an actor just says (SayTo, Say) sets no conversation and does not turn the
//! view [owner request 2026-10-10]; one said to someone turns the speaker (the engine's
//! `speech_look.rs`, dialogue-play.md §9).
//!
//! - **Opening.** When a conversation starts the view eases onto the actor's face over
//!   [`OPEN_SECONDS`] ([`smooth`], no snap).
//! - **Zoom locks, free look does not zoom** [owner decision 2026-11-13, #2254: "want it
//!   split so free look camera does not zoom and when zoom lock the camera"]: while the
//!   conversation is zoomed ([`view_locked`]) the view follows the face for the whole
//!   conversation and the mouse moves only the cursor ([`Ease::follow`]). Without the
//!   zoom only the opening turn happens and the mouse looks around as before; an ease
//!   never fights it [owner decision 2026-10-10, `crate::dialogue::FreeLook`;
//!   `Ease::step`].
//! - **Choices without the zoom.** The view does not come back by itself: after the
//!   opening turn it is the player's own for the rest of the conversation [owner decision
//!   2026-10-10: "never come back i just want the initial look at npc to happen then
//!   allow the player to free look the rest of the time"]. With
//!   [`LookSettings::recentre_on_choices`] on (`nvforever --dialogue-recentre`) the view
//!   eases back onto the face over [`RECENTRE_SECONDS`] each time the choices appear.
//! - **No roll.** Only the view's heading and pitch change here (`Player::heading`,
//!   `Player::pitch`). The zoom is `crate::dialogue_zoom`'s (the field of view and, from
//!   third person, the camera's place, dialogue-play.md §8.8); with it the turn aims a
//!   little under the face ([`crate::dialogue_zoom::face_drop`]), so the face sits above
//!   the view's centre [design: `Ease::follow`].
//! - **Pausing** (docs/design/dialogue.md §8). The eases run on the virtual clock, so the
//!   Esc pause menu holds them. With `--menu-pause original` the picture stands still
//!   for the whole conversation (the virtual clock is paused), so an ease is completed at
//!   once: the frozen picture shows the two facing each other.
//!
//! All angles and timings here are NVForever's design choices, not the original's values.
//!
//! Log, one line each, starting `dialogue look:`: the view starts turning (to whom, from
//! and to which angles), an ease dropped because the view was moved, the re-centring on
//! choices, and the end of the conversation.

use crate::dialogue_zoom::{DialogueZoom, ZoomSettings};
use crate::pause::{MenuPause, Paused};
use crate::phys::PhysState;
use crate::player::{Player, MAX_PITCH};
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::PlaySet;
use bevy::prelude::*;
use fnv_sim::FormId;
use std::f32::consts::{PI, TAU};

/// Seconds the view takes to turn to the actor when a conversation starts [design].
pub const OPEN_SECONDS: f32 = 0.4;
/// Seconds the view takes to come back onto the actor when the choices appear [design].
pub const RECENTRE_SECONDS: f32 = 0.35;
/// Choices recenter only when the view is at least this far off the face, radians
/// [design: face_speaker].
pub const ON_TARGET: f32 = 0.5 * PI / 180.0;
/// Moving-face follow time constant while zoomed, seconds; filters frame-to-frame head
/// motion after the opening ease [design, #2254].
pub const FACE_FOLLOW_SECONDS: f32 = 0.08;
/// Log final framing error when a conversation lasted at least this long [design, #2254].
pub const FRAMING_LOG_SECONDS: f32 = 6.0;
/// With a frozen picture (`--menu-pause original`) the view is put on the face at once,
/// and again on each of this many frames: the eye stands in front of the player's feet
/// and moves as the body turns with the view, so the first aim is a few degrees off as
/// seen from where the eye ends up [observed: 3° at 76° of turn, 2026-10-10].
pub const SNAP_FRAMES: u8 = 3;
/// Where the face of an actor is taken to be when nothing draws it: this share of its
/// hit volume's height above its feet [design].
pub const FACE_SHARE_OF_HEIGHT: f32 = 0.9;
/// And without a hit volume: this far above the reference's position, game units
/// [hypothesis: about the eye height of a standing human, `crate::player::EYE_HEIGHT`].
pub const FACE_ABOVE_REFERENCE: f32 = 112.0;

/// Whether the two turn to each other in a conversation (`nvforever --no-dialogue-look`
/// switches it off), and whether the view comes back onto the actor when the choices
/// appear (off unless `--dialogue-recentre`; owner decision 2026-10-10).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LookSettings {
    pub enabled: bool,
    pub recentre_on_choices: bool,
}

impl Default for LookSettings {
    fn default() -> Self {
        LookSettings { enabled: true, recentre_on_choices: false }
    }
}

/// Whether the conversation's view is locked: whenever it is zoomed. The mouse and the
/// right stick then move only the cursor, and the view follows the speaker's face (with
/// the turning on) [owner decision 2026-11-13, #2254: "want it split so free look camera
/// does not zoom and when zoom lock the camera"]. One rule for `crate::dialogue::sync_state`
/// (the cursor), `crate::player::mouse_look`, the look test aid and [`face_speaker`].
pub fn view_locked(zoom: &DialogueZoom) -> bool {
    zoom.wanted()
}

/// Where the face of the conversation's actor is drawn this frame (the reference, a
/// point in game space), written by the renderer while it draws that actor; `None`
/// otherwise (then [`face_speaker`] falls back to the actor's hit volume or position).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct SpeakerFace(pub Option<(u32, [f32; 3])>);

/// An angle brought into (−π, π].
pub fn wrap(a: f32) -> f32 {
    let w = (a + PI).rem_euclid(TAU) - PI;
    if w <= -PI {
        w + TAU
    } else {
        w
    }
}

/// The ease curve: 0 at 0, 1 at 1, no speed at either end (smoothstep).
pub fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The view (compass heading in [0, 2π), pitch) that looks from `eye` at `point`, both
/// game space; the pitch within the player's own limits.
pub fn aim(eye: [f32; 3], point: [f32; 3]) -> (f32, f32) {
    let (dx, dy, dz) = (point[0] - eye[0], point[1] - eye[1], point[2] - eye[2]);
    let flat = dx.hypot(dy);
    if flat < 1e-3 && dz.abs() < 1e-3 {
        return (0.0, 0.0);
    }
    (dx.atan2(dy).rem_euclid(TAU), dz.atan2(flat).clamp(-MAX_PITCH, MAX_PITCH))
}

/// How far apart two views are, radians (the larger of the heading and pitch differences).
pub fn apart(a: (f32, f32), b: (f32, f32)) -> f32 {
    wrap(a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// What an [`Ease`] did this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// The view to set.
    Turn((f32, f32)),
    /// The view to set; the ease is over.
    Done((f32, f32)),
    /// Something else moved the view: leave it alone; the ease is over.
    Dropped,
}

/// The view turning to a target over a fixed time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ease {
    /// The view the ease started from.
    pub from: (f32, f32),
    /// Seconds run and the ease's length.
    pub t: f32,
    pub seconds: f32,
    /// The view the ease last wrote (at first: the view it started from).
    pub last: (f32, f32),
    /// Frames the view was put on the target at once ([`SNAP_FRAMES`]).
    pub snaps: u8,
}

impl Ease {
    pub fn new(from: (f32, f32), seconds: f32) -> Ease {
        Ease { from, t: 0.0, seconds, last: from, snaps: 0 }
    }

    /// Advance as [`Ease::step`]; while the view is locked ([`view_locked`]) it never ends:
    /// after the turn it keeps on the moving face, smoothed over [`FACE_FOLLOW_SECONDS`],
    /// and nothing else writes the view meanwhile, so it is never dropped [design, #2254].
    pub fn follow(&mut self, now: (f32, f32), target: (f32, f32), dt: f32, instant: bool, locked: bool) -> Step {
        if !locked {
            return self.step(now, target, dt, instant);
        }
        self.last = now;
        if !instant && self.t >= self.seconds {
            let weight = 1.0 - (-dt.max(0.0) / FACE_FOLLOW_SECONDS).exp();
            self.last = ((now.0 + wrap(target.0 - now.0) * weight).rem_euclid(TAU), (now.1 + (target.1 - now.1) * weight).clamp(-MAX_PITCH, MAX_PITCH));
            return Step::Turn(self.last);
        }
        match self.step(now, target, dt, instant) {
            Step::Done(v) => Step::Turn(v),
            step => step,
        }
    }

    /// Advance by `dt` towards `target` (which may move: the actor's head does), given
    /// the view as it is now. `instant` puts the view on the target at once, and ends the
    /// ease after [`SNAP_FRAMES`] such frames.
    pub fn step(&mut self, now: (f32, f32), target: (f32, f32), dt: f32, instant: bool) -> Step {
        // Never fight the mouse: the view is no longer where the ease left it.
        if apart(now, self.last) > 1e-4 {
            return Step::Dropped;
        }
        if instant {
            self.t = self.seconds;
            // The locked follow keeps stepping after completion [design, #2254].
            self.snaps = self.snaps.saturating_add(1);
            self.last = (target.0.rem_euclid(TAU), target.1.clamp(-MAX_PITCH, MAX_PITCH));
            return if self.snaps >= SNAP_FRAMES { Step::Done(self.last) } else { Step::Turn(self.last) };
        }
        self.t += dt.max(0.0);
        let s = if self.seconds > 0.0 { smooth(self.t / self.seconds) } else { 1.0 };
        let heading = (self.from.0 + wrap(target.0 - self.from.0) * s).rem_euclid(TAU);
        let pitch = (self.from.1 + (target.1 - self.from.1) * s).clamp(-MAX_PITCH, MAX_PITCH);
        self.last = (heading, pitch);
        if self.t >= self.seconds {
            Step::Done(self.last)
        } else {
            Step::Turn(self.last)
        }
    }
}

/// What the face point of an actor was taken from (for the log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceFrom {
    /// The drawn actor's head ([`SpeakerFace`]).
    Head,
    /// Its hit volume ([`FACE_SHARE_OF_HEIGHT`]).
    Volume,
    /// The reference's position ([`FACE_ABOVE_REFERENCE`]).
    Reference,
    /// A talking activator's own position (an intercom on a wall, not a face).
    Activator,
}

/// The face of `actor`: the drawn head when the renderer gave one for this actor, else a
/// point near the top of its hit volume, else above its reference's position.
pub fn face_point(actor: u32, drawn: Option<(u32, [f32; 3])>, volume: Option<([f32; 3], f32)>, reference: Option<[f32; 3]>) -> Option<([f32; 3], FaceFrom)> {
    if let Some((_, p)) = drawn.filter(|(who, _)| *who == actor) {
        return Some((p, FaceFrom::Head));
    }
    if let Some((feet, height)) = volume {
        return Some(([feet[0], feet[1], feet[2] + height * FACE_SHARE_OF_HEIGHT], FaceFrom::Volume));
    }
    reference.map(|p| ([p[0], p[1], p[2] + FACE_ABOVE_REFERENCE], FaceFrom::Reference))
}

/// The conversation the view follows.
#[derive(Debug, Clone, PartialEq)]
struct Talk {
    actor: FormId,
    name: String,
    /// The opening ease was started.
    opened: bool,
    /// The choices were on screen last frame.
    choosing: bool,
    ease: Option<Ease>,
    /// Diagnostic clock and latest framing error/lock state [design, #2254].
    elapsed: f32,
    framing: Option<(f32, bool)>,
}

/// The view's side of a conversation's facing.
#[derive(Resource, Default, Debug)]
pub struct ViewLook {
    talk: Option<Talk>,
}

impl ViewLook {
    /// Whether an ease is turning the view now.
    pub fn turning(&self) -> bool {
        self.talk.as_ref().is_some_and(|t| t.ease.is_some())
    }
}

pub struct DialogueLookPlugin;

impl Plugin for DialogueLookPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LookSettings>()
            .init_resource::<SpeakerFace>()
            .init_resource::<ViewLook>()
            .init_resource::<ZoomSettings>()
            .init_resource::<DialogueZoom>()
            // After the frame's input (the mouse, the test aids), before the player's body
            // poses itself and the camera is placed.
            .add_systems(Update, (crate::dialogue_zoom::track, face_speaker).chain().in_set(PlaySet::Physics).before(crate::player::BodySet))
            .add_systems(Update, crate::dialogue_zoom::apply.in_set(PlaySet::Sync).after(crate::aim_zoom::zoom));
        if !app.world().resource::<ZoomSettings>().enabled {
            info!("dialogue zoom is off (--no-dialogue-zoom)");
        }
        let s = app.world().resource::<LookSettings>();
        if !s.enabled {
            info!("dialogue look is off (--no-dialogue-look)");
        } else if s.recentre_on_choices {
            info!("dialogue look: the view comes back onto the actor when choices appear (--dialogue-recentre)");
        }
        use crate::settings::{add_setting, Setting};
        add_setting(
            app,
            Setting {
                key: "dialogue-look",
                group: crate::pause::GROUP_MENUS,
                label: "Turn to each other in dialogue",
                note: "On: at the start of a conversation the actor turns to the player and the view turns to the actor.",
                switches: "--no-dialogue-look",
                get: |w| w.resource::<LookSettings>().enabled,
                set: |w, on| w.resource_mut::<LookSettings>().enabled = on,
                slider: None,
            },
        );
        add_setting(
            app,
            Setting {
                key: "dialogue-recentre",
                group: crate::pause::GROUP_MENUS,
                label: "Bring the view back when choices appear",
                note: "On: when choices appear the view returns to the speaker (with the close-up off). Needs \"Turn to each other in dialogue\".",
                switches: "--dialogue-recentre",
                get: |w| w.resource::<LookSettings>().recentre_on_choices,
                set: |w, on| w.resource_mut::<LookSettings>().recentre_on_choices = on,
                slider: None,
            },
        );
        add_setting(
            app,
            Setting {
                key: "dialogue-zoom",
                group: crate::pause::GROUP_MENUS,
                label: "Close in on the speaker in dialogue",
                note: "On: in a conversation the view closes in on the speaker, the background out of focus; the mouse moves only the cursor. Off: you can look around.",
                switches: "--no-dialogue-zoom",
                get: |w| w.resource::<ZoomSettings>().enabled,
                set: |w, on| w.resource_mut::<ZoomSettings>().enabled = on,
                slider: None,
            },
        );
    }
}

fn deg(a: f32) -> f32 {
    a.to_degrees()
}

/// Turn the player's view to the conversation's actor (the module notes).
#[allow(clippy::too_many_arguments)]
fn face_speaker(
    time: Res<Time>,
    settings: Res<LookSettings>,
    mode: Res<MenuPause>,
    paused: Res<Paused>,
    script: Option<Res<ScriptState>>,
    face: Res<SpeakerFace>,
    phys: Res<PhysState>,
    sim: Res<SimState>,
    mut view: ResMut<ViewLook>,
    player: Option<ResMut<Player>>,
    zoom: Res<DialogueZoom>,
) {
    let game = script.as_ref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
    let Some((game, conv)) = game.and_then(|g| g.dialogue.as_ref().map(|c| (g, c))) else {
        if let Some(t) = view.talk.take().filter(|t| t.opened) {
            if let Some((error, locked)) = t.framing.filter(|_| t.elapsed >= FRAMING_LOG_SECONDS) {
                info!("dialogue look: framing after {:.1} s: {} offset {:.3} degrees (locked={locked})", t.elapsed, t.name, error);
            }
            info!("dialogue look: the conversation with {} is over: the view is the player's own again", t.name);
        }
        return;
    };
    let Some(mut player) = player else { return };
    // The pause menu holds everything.
    if !settings.enabled || paused.0 {
        return;
    }
    // Through a talking activator the view turns to the activator, not to the actor
    // speaking from elsewhere (`fnv_game::talking_activators`).
    let actor = conv.through.unwrap_or(conv.actor);
    if view.talk.as_ref().is_none_or(|t| t.actor != actor) {
        view.talk = Some(Talk { actor, name: game.data.name(actor), opened: false, choosing: false, ease: None, elapsed: 0.0, framing: None });
    }
    let talk = view.talk.as_mut().expect("set above");
    talk.elapsed += time.delta_secs();
    // The choices are on screen (not under a message box, which takes the keys first).
    let choosing = conv.line.is_none() && !conv.choices.is_empty() && !crate::dialogue::held(game);
    let volume = phys.world.actor_volume(actor.0).map(|v| (v.feet, v.height));
    let reference = sim.world.peek(actor).map(|s| s.pos).or_else(|| sim.refs.get(actor).map(|p| p.pos));
    let found = match conv.through {
        Some(_) => reference.map(|p| (p, FaceFrom::Activator)),
        None => face_point(actor.0, face.0, volume, reference),
    };
    let Some((point, from)) = found else {
        if !talk.opened {
            talk.opened = true;
            info!("dialogue look: {} is nowhere to be found: the view stays", talk.name);
        }
        talk.choosing = choosing;
        return;
    };
    let now = (player.heading, player.pitch);
    // With a zoom the face sits above the view's centre (`crate::dialogue_zoom`).
    let target = aim(player.eye(), point);
    let target = (target.0, target.1 - zoom.face_drop());
    let locked = view_locked(&zoom);
    if !talk.opened {
        talk.opened = true;
        talk.ease = Some(Ease::new(now, OPEN_SECONDS));
        info!(
            "dialogue look: the view turns to {} ({}): heading {:.1}° to {:.1}°, pitch {:.1}° to {:.1}°, over {OPEN_SECONDS:.2} s",
            talk.name,
            match from {
                FaceFrom::Head => "its head as drawn",
                FaceFrom::Volume => "the top of its hit volume: it has no drawn head",
                FaceFrom::Reference => "above its reference: it is not drawn",
                FaceFrom::Activator => "the talking activator itself",
            },
            deg(now.0),
            deg(target.0),
            deg(now.1),
            deg(target.1)
        );
    } else if talk.ease.is_none() && locked {
        // A zoom that came after the opening turn: the locked view comes onto the face.
        talk.ease = Some(Ease::new(now, RECENTRE_SECONDS));
        info!(
            "dialogue look: the zoom locks the view on {}: heading {:.1}° to {:.1}°, pitch {:.1}° to {:.1}°, over {RECENTRE_SECONDS:.2} s",
            talk.name,
            deg(now.0),
            deg(target.0),
            deg(now.1),
            deg(target.1)
        );
    } else if talk.ease.is_none() && choosing && !talk.choosing && settings.recentre_on_choices && apart(now, target) > ON_TARGET {
        talk.ease = Some(Ease::new(now, RECENTRE_SECONDS));
        info!(
            "dialogue look: choices: the view comes back onto {}: heading {:.1}° to {:.1}°, pitch {:.1}° to {:.1}°, over {RECENTRE_SECONDS:.2} s",
            talk.name,
            deg(now.0),
            deg(target.0),
            deg(now.1),
            deg(target.1)
        );
    }
    talk.choosing = choosing;
    // With `--menu-pause original` the picture stands still: no ease could be seen.
    let instant = *mode == MenuPause::Original;
    if let Some(ease) = talk.ease.as_mut() {
        match ease.follow(now, target, time.delta_secs(), instant, locked) {
            Step::Turn(v) => (player.heading, player.pitch) = v,
            Step::Done(v) => {
                (player.heading, player.pitch) = v;
                debug!("dialogue look: the view is on {}: heading {:.1}°, pitch {:.1}°", talk.name, deg(v.0), deg(v.1));
                talk.ease = None;
            }
            Step::Dropped => {
                info!("dialogue look: the view was moved (the mouse, or a test aid): it is left where the player put it");
                talk.ease = None;
            }
        }
    }
    talk.framing = zoom.wanted().then(|| (apart((player.heading, player.pitch), target).to_degrees(), locked));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn aiming_gives_the_compass_heading_and_the_pitch() {
        // North, east, south-west; up and down.
        let (h, p) = aim([0.0, 0.0, 100.0], [0.0, 50.0, 100.0]);
        assert!(close(h, 0.0) && close(p, 0.0));
        let (h, _) = aim([0.0, 0.0, 0.0], [50.0, 0.0, 0.0]);
        assert!(close(h, FRAC_PI_2));
        let (h, _) = aim([0.0, 0.0, 0.0], [-10.0, -10.0, 0.0]);
        assert!(close(h, 1.25 * PI), "headings are in [0, 2π): {h}");
        let (_, p) = aim([0.0, 0.0, 0.0], [0.0, 100.0, 100.0]);
        assert!(close(p, PI / 4.0));
        let (_, p) = aim([0.0, 0.0, 100.0], [0.0, 1.0, -1.0e6]);
        assert_eq!(p, -MAX_PITCH, "never past straight down");
        // The same view the camera is built from.
        let d = crate::space::look_dir(1.0, -0.3);
        let (h, p) = aim([5.0, 6.0, 7.0], [5.0 + d[0] * 80.0, 6.0 + d[1] * 80.0, 7.0 + d[2] * 80.0]);
        assert!(close(h, 1.0) && close(p, -0.3));
    }

    #[test]
    fn an_ease_turns_the_short_way_without_a_snap() {
        // From heading 350° to 20°: through north, 30° in all.
        let from = (350f32.to_radians(), 0.2);
        let target = (20f32.to_radians(), -0.1);
        let mut e = Ease::new(from, 0.4);
        let mut now = from;
        let mut last_step = 0.0f32;
        let mut seen = Vec::new();
        for i in 0..40 {
            match e.step(now, target, 0.01, false) {
                Step::Turn(v) | Step::Done(v) => {
                    let moved = wrap(v.0 - now.0);
                    assert!(moved >= -1e-6, "always clockwise through north: frame {i} moved {moved}");
                    // No snap: the first and last steps are the smallest.
                    seen.push(moved);
                    last_step = moved;
                    now = v;
                }
                Step::Dropped => panic!("nothing moved the view"),
            }
        }
        assert!(apart(now, target) < 1e-4, "{now:?}");
        let biggest = seen.iter().copied().fold(0.0, f32::max);
        assert!(seen[0] < biggest / 5.0 && last_step < biggest / 5.0, "eased in and out: {seen:?}");
        assert!(biggest < 3f32.to_radians(), "no step is a jump: {}", biggest.to_degrees());
        assert!(matches!(Ease::new(from, 0.4).step(from, target, 0.5, false), Step::Done(_)));
    }

    #[test]
    fn an_ease_never_fights_the_mouse() {
        let mut e = Ease::new((1.0, 0.0), 0.4);
        let Step::Turn(v) = e.step((1.0, 0.0), (2.0, 0.3), 0.1, false) else { panic!() };
        // The mouse moved the view since: the ease lets go.
        assert_eq!(e.step((v.0 + 0.02, v.1), (2.0, 0.3), 0.1, false), Step::Dropped);
        // Untouched, it goes on.
        let mut e = Ease::new((1.0, 0.0), 0.4);
        let Step::Turn(v) = e.step((1.0, 0.0), (2.0, 0.3), 0.1, false) else { panic!() };
        assert!(matches!(e.step(v, (2.0, 0.3), 0.1, false), Step::Turn(_)));
    }

    #[test]
    fn the_zoomed_lock_follows_a_face_after_six_seconds_and_is_never_dropped() {
        let mut e = Ease::new((1.0, 0.0), OPEN_SECONDS);
        let mut now = e.from;
        for frame in 0..480 {
            // Synthetic moving face, starts walking well after the old 1.6 s hold
            // [design, #2254].
            let target = (2.0 + ((frame as f32 / 60.0 - 2.0) * 0.05).max(0.0), 0.1);
            let Step::Turn(v) = e.follow(now, target, 1.0 / 60.0, false, true) else { panic!("follow ended at frame {frame}") };
            now = v;
            if frame > 360 {
                assert!(apart(now, target).to_degrees() < 0.25, "{now:?}, {target:?}");
            }
        }
        // Another writer moved the view: the lock takes it back, it does not let go.
        let moved = (now.0 + 0.02, now.1);
        assert!(matches!(e.follow(moved, (2.4, 0.1), 0.1, false, true), Step::Turn(_)));
    }

    #[test]
    fn zoom_off_keeps_the_opening_only_and_mouse_takeover() {
        assert!(!view_locked(&DialogueZoom::default()));
        assert!(view_locked(&DialogueZoom::for_test_conversation()));
        let mut e = Ease::new((1.0, 0.0), OPEN_SECONDS);
        assert!(matches!(e.follow(e.from, (2.0, 0.1), 0.5, false, false), Step::Done(_)));
        let mut e = Ease::new((1.0, 0.0), OPEN_SECONDS);
        assert_eq!(e.follow((1.02, 0.0), (2.0, 0.1), 0.1, false, false), Step::Dropped);
    }

    #[test]
    fn follow_filters_motion_and_wraps_the_short_way() {
        let mut e = Ease::new((359f32.to_radians(), 0.0), 0.0);
        let Step::Turn(v) = e.follow(e.from, (1f32.to_radians(), 0.0), 0.01, false, true) else { panic!() };
        assert!(wrap(v.0 - e.from.0) > 0.0);
        assert!(apart(v, e.from) < 1f32.to_radians());
        assert!(apart(v, (1f32.to_radians(), 0.0)) > 1f32.to_radians());
        assert_eq!(e.follow(v, (0.1, 0.3), 0.0, false, true), Step::Turn(v));
    }

    fn mouse_app(zoomed: bool) -> App {
        use bevy::input::mouse::AccumulatedMouseMotion;
        use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
        let mut app = App::new();
        app.insert_resource(State::new(crate::PlayState::Dialogue))
            .insert_resource(Time::<()>::default())
            .init_resource::<crate::bindings::Bindings>()
            .init_resource::<crate::control::ControlLocks>()
            .init_resource::<crate::control::PlayerControl>()
            .init_resource::<crate::furniture::PlayerSeat>()
            .insert_resource(crate::dialogue::FreeLook(true))
            .insert_resource(if zoomed { DialogueZoom::for_test_conversation() } else { DialogueZoom::default() })
            .insert_resource(AccumulatedMouseMotion { delta: Vec2::ZERO })
            .insert_resource(Player { heading: 1.0, pitch: 0.0, eye_height: 112.0, feet: [0.0; 3], body_eye: None, body_view: None, placed: None })
            .add_systems(Update, crate::player::mouse_look);
        app.world_mut().spawn((PrimaryWindow, CursorOptions { grab_mode: CursorGrabMode::Confined, ..default() }));
        app.update();
        app.update();
        app
    }

    #[test]
    fn mouse_motion_turns_the_view_only_without_the_zoom() {
        use bevy::input::mouse::AccumulatedMouseMotion;
        for zoomed in [true, false] {
            let mut app = mouse_app(zoomed);
            app.world_mut().resource_mut::<AccumulatedMouseMotion>().delta = Vec2::new(80.0, 20.0);
            app.update();
            let p = app.world().resource::<Player>();
            assert_eq!(p.heading == 1.0 && p.pitch == 0.0, zoomed, "zoomed={zoomed}");
        }
    }

    #[test]
    fn cursor_release_and_regrab_motion_do_not_take_over_the_view() {
        use bevy::input::mouse::AccumulatedMouseMotion;
        use bevy::window::{CursorGrabMode, CursorOptions};
        // Free look, without the zoom: the grab at each line and the release at the
        // choices are not taken as the player moving the mouse.
        let mut app = mouse_app(false);
        let mut cursors = app.world_mut().query_filtered::<Entity, With<CursorOptions>>();
        let entity = cursors.single(app.world()).unwrap();
        app.world_mut().resource_mut::<AccumulatedMouseMotion>().delta = Vec2::new(80.0, 20.0);
        for mode in [CursorGrabMode::None, CursorGrabMode::Confined] {
            app.world_mut().get_mut::<CursorOptions>(entity).unwrap().grab_mode = mode;
            app.update();
            app.update();
            assert_eq!(app.world().resource::<Player>().heading, 1.0, "after {mode:?}");
        }
        app.update();
        assert_ne!(app.world().resource::<Player>().heading, 1.0, "subsequent real motion is accepted");
    }

    #[test]
    fn there_is_no_lock_setting_beside_the_zoom() {
        let mut app = App::new();
        app.add_plugins(DialogueLookPlugin);
        let settings = app.world().resource::<crate::settings::Settings>();
        assert!(settings.index("dialogue-zoom").is_some());
        assert!(settings.index("dialogue-lock").is_none(), "the zoom is the lock [owner decision 2026-11-13]");
    }

    #[test]
    fn a_frozen_picture_gets_the_end_of_the_ease_at_once() {
        let mut e = Ease::new((0.0, 0.0), 0.4);
        // No virtual time passes under `--menu-pause original`: on the target at once.
        let Step::Turn(v) = e.step((0.0, 0.0), (3.0, -0.5), 0.0, true) else { panic!("over too soon") };
        assert!(apart(v, (3.0, -0.5)) < 1e-5, "{v:?}");
        // The eye moved with the turn: the target as seen from there is taken up too,
        // and then the ease is over.
        let Step::Turn(v) = e.step(v, (2.95, -0.5), 0.0, true) else { panic!("over too soon") };
        let Step::Done(v) = e.step(v, (2.94, -0.5), 0.0, true) else { panic!("not over") };
        assert!(apart(v, (2.94, -0.5)) < 1e-5, "{v:?}");
        // And with the clock merely stopped (the pause menu) nothing moves.
        let mut e = Ease::new((0.0, 0.0), 0.4);
        assert_eq!(e.step((0.0, 0.0), (3.0, -0.5), 0.0, false), Step::Turn((0.0, 0.0)));
    }

    #[test]
    fn frozen_zoom_follows_for_more_than_256_frames() {
        let mut e = Ease::new((0.0, 0.0), OPEN_SECONDS);
        let mut now = e.from;
        for frame in 0..1024 {
            // Synthetic moving target with no virtual time passing [design, #2254].
            let target = (3.0 + frame as f32 * 0.001, -0.5);
            let Step::Turn(v) = e.follow(now, target, 0.0, true, true) else { panic!("frozen follow ended at frame {frame}") };
            assert!(apart(v, target) < 1e-5, "frame {frame}: {v:?}");
            now = v;
        }
        // The snap counter saturated rather than wrapping (a wrap passes in release).
        assert_eq!(e.snaps, u8::MAX);
    }

    #[test]
    fn the_face_is_the_drawn_head_else_the_volume_else_the_reference() {
        let head = Some((7, [1.0, 2.0, 3.0]));
        assert_eq!(face_point(7, head, Some(([0.0; 3], 128.0)), Some([9.0; 3])), Some(([1.0, 2.0, 3.0], FaceFrom::Head)));
        // A head drawn for another actor is not this one's.
        let (p, from) = face_point(8, head, Some(([10.0, 20.0, 30.0], 100.0)), None).unwrap();
        assert_eq!(from, FaceFrom::Volume);
        assert!(close(p[2], 30.0 + 100.0 * FACE_SHARE_OF_HEIGHT) && p[0] == 10.0);
        let (p, from) = face_point(8, None, None, Some([1.0, 1.0, 1.0])).unwrap();
        assert_eq!((p, from), ([1.0, 1.0, 1.0 + FACE_ABOVE_REFERENCE], FaceFrom::Reference));
        assert_eq!(face_point(8, None, None, None), None);
    }

    #[test]
    fn angles_wrap() {
        assert!(close(wrap(3.0 * PI), PI) && close(wrap(-PI), PI) && close(wrap(0.1 - TAU), 0.1));
        assert!(close(apart((0.1, 0.0), (TAU - 0.1, 0.05)), 0.2));
        assert_eq!((smooth(0.0), smooth(0.5), smooth(1.0), smooth(2.0)), (0.0, 0.5, 1.0, 1.0));
    }
}
