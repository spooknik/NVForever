//! Kill cameras (docs/design/vats-mode.md §19): when the last marked shot of a VATS-mode
//! run kills its actor, the original's camera shots (CAMS) chosen by its camera paths
//! (CPTH) play, as VATS playback's kill cameras do [documented: GECK wiki CameraShot,
//! "cameras for VATS playback"; the paths and shots are the masters' own].
//!
//! - **When** [design: VATS mode fires its marks in one run (§9), so a kill camera only
//!   follows the run's last mark, not to cut the run short]: from the frame every mark's
//!   attack has started until [`WATCH_AFTER`] game seconds after the mode is off, the last
//!   mark's actor is watched; if it dies, the kill camera starts. An actor already dead
//!   when watched is not.
//! - **Which** (`fnv_game::camera_paths`): the masters' path tree, its conditions run on
//!   the player with GetVATSValue answering about the last shot, known to be fatal
//!   (`Game::kill_outcome`), and the VATS area functions on [`CameraRoom`] measured here
//!   by rays ([`measure_room`]). The shots the path starts on the hit play in turn
//!   (`camera_paths::hit_shots`).
//! - **A shot**: plays for its Max Time in real seconds, no less than [`SHOT_LEAST`]
//!   [hypothesis: "the max time the camera will playback before switching to the next
//!   camera", GECK wiki CameraShot; with nothing to cut to sooner, the most]; the world's
//!   clock runs at its Global time multiplier (the player's and the target's own are not
//!   applied: one clock for the world) and its image space modifier (MNAM) plays.
//! - **Where the camera is** [hypothesis: the shot models hold no reference frame; this is
//!   the frame in which their cameras stand where the shot names say, a front shot in
//!   front of the target and so on; observed on `simplefronthitl03.nif`, at (−59, −226,
//!   18) for a front hit, and `snipercamhit01.nif`]: the model's animated root, in a frame
//!   centred on the actor the shot is held by (its Location: the target or the attacker;
//!   a projectile counts as the attacker) at its middle (the aim point), with +Y the level
//!   direction from the attacker to the target and +Z up. With Position Follows Location
//!   the frame follows the actor, else it stays where it was when the shot began. The
//!   camera looks at its Target actor's middle, moved Target % Between Actors of the way
//!   to the other actor, followed while Rotation Follows Target is set, else as at the
//!   start; the model's own rotation is added as a sway (its turn since its first key). A
//!   First Person Camera shot leaves the view where it is. The camera is kept in front of
//!   what lies between the look point and it: the world, not the actors ([`ray_filter`];
//!   the rays took the actors' own capsules for walls until 2026-11-10, the player's
//!   too, so a camera behind the attacker was pulled to just in front of them: a view
//!   of the weapon and nothing of the attacker [observed: the fork owner's playtest,
//!   "close up of the weapons and not of the target and the player"], and the room
//!   measured about them was their capsule's width).
//! - **The player's own speed** [documented: GECK wiki CameraShot, "Player Multiplier:
//!   how fast the player moves in comparison to the modified global time"; observed: the
//!   masters' shooting shots slow the world to about an eighth and give the player 8 to
//!   10 times that, so the attacker moves at about its own speed in a world that
//!   crawls]: while a shot plays the player's body, its weapon's cycle and VATS mode's
//!   marks go at the shot's Player multiplier times the world's clock
//!   ([`KillCam::player_scale`], no faster than [`PLAYER_FASTEST`] of real time). The
//!   Target multiplier is not applied (no actor but the player has a clock of its own).
//! - While it plays the view counts as a third-person one (`PlayerControl::cinematic`):
//!   the player's body is drawn and the first-person arms are not.
//! - It ends early when the player dies, a menu opens or the game is not playing.
//! - **Only the last enemy** [owner's playtest request #1035; design: the original plays
//!   one on any killing VATS shot, hypothesis; combat is not a parity target]: with the
//!   settings page's Kill camera at Last enemy (the default, [`KillCamSetting`]), a kill
//!   that leaves another living actor in combat with the player ([`enemies_in_combat`],
//!   the player's IsInCombat rule: an actor in a combat state, Alert, Combat or Lost, with
//!   the player on its target or alert list) shows none, and the view stays the player's.
//!   Every kill shows one whoever is left; Off never shows one (#1037).
//! - **While it plays** (#1034, #1036) [design]: the HUD, the crosshair and the corner
//!   and quest notices are hidden (`hud::gather`, `reticle::draw`, the tile HUD,
//!   `corner_messages`, `quest_messages`; the notices wait, as under a message box), and
//!   the gameplay controls are held: the keys and buttons read through
//!   `bindings::ControlInput` other than Menu Mode, Wait, Quick Save and Quick Load (so
//!   the game can still be paused or a menu opened, which ends the camera), the mouse and
//!   stick look (`player::mouse_look`, the look test aid), the weapon wheel, and what the
//!   frame asked of the character: moving, jumping, the trigger, aiming, reload and ammo
//!   swap ([`hold_input`], which the test aids that stand in for the keys go through). A
//!   fire button held when it starts must be pressed again after it (the trigger needs a
//!   fresh press, `control::read_controls`).
//!
//! - **The classic style's attack cameras** (docs/design/vats-classic.md §8): while the
//!   classic style plays its queue, each attack asks for the path's shots started by Shoot
//!   and, but for the last attack (whose kill is this module's own camera), by Hit after
//!   ([`VatsMode::camera_ask`]). They play as a kill camera's do, for [`attack_length`]
//!   each. The attack leaves a moment into its Shoot camera and is seen in it: the last
//!   Shoot shot stays until the attack is out whole (`VatsMode::attack_out`: a burst's
//!   rounds, a throw's wind-up) and [`SHOOT_AFTER`] more, no longer than
//!   [`SHOOT_PATIENCE`]; the next attack waits for the cameras
//!   (`vats_mode::classic_cameras`). Their lines start `vats camera:`.
//! - **Where an attack's camera stands: three angles** ([`Angle`], [`angle_spot`]) [design:
//!   the fork owner 2026-11-10, "update the camera angle so it has these 4 options: both
//!   player character and target npc in frame, first person view, only the targeted npc,
//!   only the player character; vats switch between them but not too fast so the camera
//!   angles don't just jump around for each shot"; the fourth, the player alone, went the
//!   same day: first "the killshot should always have the NPC kill visible therefore
//!   exclude cam 4", then "remove the player only camera all together"]. The path's shot still gives an
//!   attack's camera its length and its clocks (the world's and the player's); where the
//!   camera stands is one of the four, not the shot's model, and its image space modifier
//!   is not played (it would start again with every attack). Each stay's attacks start
//!   on the next angle of [`Angle::CYCLE`]; within a stay the angle stays for
//!   [`ANGLE_DWELL`] real seconds of camera before the next one takes over, at an
//!   attack's start, never in the middle of one. Between two attacks' cameras the last
//!   angle holds the view ([`ANGLE_LINGER`]), so it does not fall back to the eye for a
//!   frame. The side of the line from the attacker to the target the camera stands on is
//!   the one with more room. A path with no Shoot shot gets NVForever's own
//!   ([`own_shot`]). The kill camera keeps the masters' shots and their models.
//!
//! Log lines: `kill camera: <name> <ref> killed by the last mark: path <EDID> (<FormID>),
//! shots <EDID>, …` when it starts (or `kill camera: <ref> killed by the last mark: no
//! camera path chosen`), `kill camera: skipped, <n> enemies still in combat (…)` or `kill
//! camera: off in the settings` for a kill that shows none, `kill camera: shot <k> of <n>
//! <EDID>: …` for each shot, and `kill camera: done (…), input held <n> frames` when it
//! ends; at exit `kill camera summary: <n> shown, <n> skipped, <n> off`.

use crate::control::PlayerControl;
use crate::player::MoveIntent;
use crate::phys::PhysState;
use crate::player::Player;
use crate::script::ScriptState;
use crate::vats_mode::{CamStage, Phase, VatsMode};
use crate::{space, PlayState, PlayerCamera};
use bevy::prelude::*;
use fnv_game::camera_paths::{self, CameraShotDef, ShotAction, ShotActor, FIRST_PERSON_CAMERA, POSITION_FOLLOWS_LOCATION, ROTATION_FOLLOWS_TARGET};
use fnv_game::vats::{CameraRoom, VatsShot};
use fnv_sim::FormId;
use nvf_assets::camera_shot::CameraModel;
use nvf_assets::model::ModelCtx;
use std::sync::Arc;

/// Game seconds after VATS mode is off that the last mark's actor is still watched (a
/// shot still in flight) [design].
pub const WATCH_AFTER: f32 = 2.0;
/// The least real seconds a shot plays [design].
pub const SHOT_LEAST: f32 = 0.5;
/// The most real seconds a shot plays [design: no shot of the masters asks for more than
/// 10].
pub const SHOT_MOST: f32 = 10.0;
/// How far the room about the actors is measured, units [design: past the largest
/// distance the masters' paths compare with, 320, observed].
pub const ROOM_REACH: f32 = 1024.0;
/// The camera keeps this far in front of what it would go into, units [design].
pub const WALL_MARGIN: f32 = 8.0;
/// The slowest the world's clock is taken [design].
pub const SLOWEST: f32 = 0.02;

/// A shot playing.
struct Playing {
    def: CameraShotDef,
    model: Option<Arc<CameraModel>>,
    /// Real seconds it has played, and for how long it plays.
    real: f32,
    length: f32,
    /// Real seconds since the attack it shows was out whole (an attack's Shoot shot).
    out_for: f32,
    /// The frame's origin and +Y when it began (game space), the look point then, and the
    /// model's first turn.
    origin: Vec3,
    forward: Vec3,
    look: Vec3,
    first_turn: Quat,
    /// The image space modifier it applied.
    imod: Option<FormId>,
}

/// What a show is: a kill's camera, or one stage of a classic attack's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Kill,
    Attack(CamStage),
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Kind::Kill => "kill camera",
            Kind::Attack(_) => "vats camera",
        }
    }
}

/// The most real seconds a shot of an attack's camera plays before the attack leaves
/// (Shoot) and after it (Hit) [design: the original cuts on the attack's own events, which
/// the shots' Min and Max Time only bound; these keep a queue of several attacks moving].
pub const ATTACK_SHOOT_MOST: f32 = 1.2;
pub const ATTACK_HIT_MOST: f32 = 1.8;
/// An attack's last Shoot shot stays this many real seconds after the attack has left
/// whole, and no shot of it longer than [`SHOOT_PATIENCE`] in all [design].
pub const SHOOT_AFTER: f32 = 0.45;
pub const SHOOT_PATIENCE: f32 = 8.0;
/// The player is no faster than this share of real time under a shot's Player time
/// multiplier [design: the masters' shots come to 0.5 to 1.1 but for a few].
pub const PLAYER_FASTEST: f32 = 1.1;

/// The cameras that show a hit (the classic style's) and the kill camera (either style's)
/// hold their last shot this many real seconds longer [design: the fork owner
/// 2026-11-10, "increase duration of the vats shooting/kill slowmo camera by 2 seconds",
/// and for Dead Eye "port all the fixes we put in vats into deadeye also"].
pub const CLASSIC_HOLD: f32 = 2.0;
/// The kill camera holds its last shot this many real seconds longer still [design: the
/// fork owner 2026-11-10, "extend the vats kill/slowmo camera by 1 second"].
pub const KILL_HOLD: f32 = 1.0;

/// Where an attack's camera stands (the module's notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Angle {
    /// Behind the attacker's shoulder: the attacker and the target both in the frame.
    Both,
    /// The eye's own view.
    FirstPerson,
    /// By the target, on the attacker's side of it: the target alone.
    Target,
}

impl Angle {
    /// The order the angles come in. Each shows the target, so a kill is always seen.
    pub const CYCLE: [Angle; 3] = [Angle::Both, Angle::Target, Angle::FirstPerson];

    fn name(self) -> &'static str {
        match self {
            Angle::Both => "both in the frame",
            Angle::FirstPerson => "first person",
            Angle::Target => "the target alone",
        }
    }
}

/// An angle stays for this many real seconds of camera within one stay's attacks before
/// the next takes over, and holds the view this long between two attacks' cameras
/// [design].
pub const ANGLE_DWELL: f32 = 6.0;
pub const ANGLE_LINGER: f32 = 0.25;

/// Where the camera of `angle` stands and where it looks (game space), on `side` of the
/// line from the attacker to the target (1 its right, -1 its left); `None` for the eye's
/// own view. `attacker` and `target` are the actors' middles, `chest` the attacker's look
/// point. Units: a person stands some 128 high [design, by eye]:
/// - both: 105 behind the attacker's middle, 42 to the side and 32 up, looking a fifth of
///   the way from the target back to the attacker;
/// - the target alone: on the attacker's side of the target, 35 degrees off the line (70
///   with the two nearer than 180, where there is no room between them), a little over
///   half their distance away, no nearer than 110 and no further than 240, 18 up.
pub fn angle_spot(angle: Angle, side: f32, attacker: Vec3, chest: Vec3, target: Vec3) -> Option<(Vec3, Vec3)> {
    let forward = level_forward(attacker, target);
    let right = forward.cross(Vec3::Z);
    match angle {
        Angle::FirstPerson => None,
        Angle::Both => Some((attacker - forward * 105.0 + right * side * 42.0 + Vec3::Z * 32.0, target + (chest - target) * 0.2)),
        Angle::Target => {
            let apart = Vec3::new(target.x - attacker.x, target.y - attacker.y, 0.0).length();
            let off = if apart < 180.0 { 70.0_f32 } else { 35.0_f32 }.to_radians();
            let away = (apart * 0.55).clamp(110.0, 240.0);
            Some((target + (right * side * off.sin() - forward * off.cos()) * away + Vec3::Z * 18.0, target))
        }
    }
}

/// The side with more room for `angle`'s camera: the left only where the right is cut
/// shorter by what stands in the way.
fn roomier_side(phys: &mut PhysState, angle: Angle, attacker: Vec3, chest: Vec3, target: Vec3) -> f32 {
    let mut kept = |side: f32| match angle_spot(angle, side, attacker, chest, target) {
        Some((want, look)) => (clear(phys, look, want) - look).length() / (want - look).length().max(1e-3),
        None => 1.0,
    };
    let (right, left) = (kept(1.0), kept(-1.0));
    if left > right + 0.05 {
        -1.0
    } else {
        1.0
    }
}

/// NVForever's own shot for an attack whose path names none: the clocks and the length
/// of the masters' common shooting shots [observed: `CamDefaultShoot…`, the world at about
/// an eighth, the player eight times that, 0.4 to 1.5 s].
pub fn own_shot() -> CameraShotDef {
    CameraShotDef {
        model: None,
        action: ShotAction::Shoot,
        location: ShotActor::Attacker,
        target: ShotActor::Target,
        flags: POSITION_FOLLOWS_LOCATION | ROTATION_FOLLOWS_TARGET,
        time_player: 8.0,
        time_target: 1.0,
        time_global: 0.12,
        max_time: 1.5,
        min_time: 0.6,
        target_pct: 0.0,
        imod: None,
    }
}

/// The angle that holds the view between two attacks' cameras.
#[derive(Debug, Clone, Copy)]
struct Linger {
    angle: Angle,
    side: f32,
    target: FormId,
    left: f32,
}

/// How long a shot of an attack's camera plays, real seconds: its Min Time when it has
/// one, else its Max Time, within [`SHOT_LEAST`] and the stage's most.
pub fn attack_length(def: &CameraShotDef, stage: CamStage) -> f32 {
    let most = match stage {
        CamStage::Shoot => ATTACK_SHOOT_MOST,
        CamStage::Hit => ATTACK_HIT_MOST,
    };
    let asked = if def.min_time > 0.0 { def.min_time } else { def.max_time };
    asked.clamp(SHOT_LEAST, most)
}

/// A camera's place for its target closes this share of the way to where the target is
/// in each of these real seconds (1 - 1/e), and goes there at once when that is further
/// than [`FOLLOW_JUMP`] units (a target put somewhere else) [design].
pub const FOLLOW_SECONDS: f32 = 0.1;
pub const FOLLOW_JUMP: f32 = 200.0;

/// A kill camera under way.
struct Show {
    kind: Kind,
    target: FormId,
    shots: Vec<FormId>,
    next: usize,
    playing: Option<Playing>,
    real: f32,
    /// Frames in which [`hold_input`] struck something the player asked for.
    held: u32,
    /// An attack's camera: its angle and side ([`angle_spot`]).
    angle: Option<(Angle, f32)>,
    /// The shot played when the path names none ([`own_shot`]).
    fallback: Option<CameraShotDef>,
    /// Where its target is for the camera: [`middle`], followed smoothly
    /// ([`FOLLOW_SECONDS`]). The camera stays about that place when the target cannot
    /// be found, not about the attacker.
    seen: Option<Vec3>,
}

/// The last mark's actor being watched.
struct Watch {
    shot: VatsShot,
    /// Game seconds since VATS mode went off (while it is on, 0).
    since_off: f32,
}

#[derive(Resource, Default)]
pub struct KillCam {
    watch: Option<Watch>,
    show: Option<Show>,
    /// Kill cameras shown, and kills that showed none (others in combat; the setting
    /// off), for the exit report.
    shown: u32,
    skipped: u32,
    off: u32,
    /// The last attack-camera request seen (`VatsMode::camera_ask`'s serial), and the
    /// attack cameras shown.
    asked: u64,
    attacks: u32,
    /// The attacks' angle now (its place in [`Angle::CYCLE`]) and side, the real seconds
    /// of camera it has had in this stay, and what holds the view between two cameras.
    angle: Option<usize>,
    angle_side: f32,
    angle_for: f32,
    linger: Option<Linger>,
}

/// When kill cameras play: the settings page's Kill camera (`kill-camera` in
/// NVForever.ini) [owner's playtest requests #1035, #1037].
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KillCamSetting {
    /// Only for a kill that leaves no other actor in combat with the player.
    #[default]
    LastEnemy,
    /// For every kill by the last mark.
    EveryKill,
    /// Never.
    Off,
}

impl KillCamSetting {
    pub const ALL: [KillCamSetting; 3] = [KillCamSetting::LastEnemy, KillCamSetting::EveryKill, KillCamSetting::Off];

    pub fn name(self) -> &'static str {
        match self {
            KillCamSetting::LastEnemy => "Last enemy",
            KillCamSetting::EveryKill => "Every kill",
            KillCamSetting::Off => "Off",
        }
    }

    /// The setting of this name, any case.
    pub fn by_name(name: &str) -> Option<KillCamSetting> {
        KillCamSetting::ALL.into_iter().find(|s| s.name().eq_ignore_ascii_case(name.trim()))
    }
}

fn setting_pick(w: &mut World, name: &str) -> bool {
    match KillCamSetting::by_name(name) {
        Some(s) => {
            *w.resource_mut::<KillCamSetting>() = s;
            true
        }
        None => false,
    }
}

/// The living actors other than `killed` in combat with the player: in a combat state
/// (Alert, Combat, Lost) with the player on their target or alert list, as the player's
/// IsInCombat counts them (`fnv_game::awareness::is_in_combat`).
pub fn enemies_in_combat(g: &fnv_game::Game, killed: FormId) -> Vec<FormId> {
    let player = FormId::PLAYER_REF;
    g.awareness
        .actors
        .iter()
        .filter(|(id, a)| **id != killed && **id != player && a.state.in_combat() && (a.targets.contains_key(&player) || a.alarms.iter().any(|x| x.owner == player)))
        .map(|(id, _)| *id)
        .filter(|id| !g.is_dead(*id))
        .collect()
}

impl KillCam {
    /// A kill camera has the view.
    pub fn active(&self) -> bool {
        self.show.is_some()
    }

    /// The newest attack-camera request seen: `vats_mode` waits until its own is.
    pub fn handled(&self) -> u64 {
        self.asked
    }

    /// An attack's camera is playing.
    pub fn attack_playing(&self) -> bool {
        self.show.as_ref().is_some_and(|s| matches!(s.kind, Kind::Attack(_)))
    }

    /// The world clock's speed it asks for (`script::time_multiplier`): the playing
    /// shot's Global time multiplier, else 1.
    pub fn time_scale(&self) -> Option<f32> {
        let p = self.show.as_ref()?.playing.as_ref()?;
        Some(if p.def.time_global > 0.0 { p.def.time_global.clamp(SLOWEST, 1.0) } else { 1.0 })
    }

    /// How much faster than the world's clock the player goes while the playing shot
    /// plays (the module's notes): its Player time multiplier, held to [`PLAYER_FASTEST`]
    /// of real time.
    pub fn player_scale(&self) -> Option<f32> {
        let world = self.time_scale()?;
        let p = self.show.as_ref()?.playing.as_ref()?;
        Some(player_scale_of(p.def.time_player, world))
    }
}

/// The player's speed against a world slowed to `world` under a shot's Player time
/// multiplier `asked` (absent or 0: the world's own).
pub fn player_scale_of(asked: f32, world: f32) -> f32 {
    let asked = if asked > 0.0 { asked } else { 1.0 };
    asked.min(PLAYER_FASTEST / world.max(SLOWEST)).max(0.01)
}

/// How long a shot plays, real seconds.
pub fn shot_length(def: &CameraShotDef) -> f32 {
    def.max_time.max(def.min_time).clamp(SHOT_LEAST, SHOT_MOST)
}

/// The level unit direction from `from` to `to` (game space); +Y when they stand one
/// over the other.
pub fn level_forward(from: Vec3, to: Vec3) -> Vec3 {
    let d = Vec3::new(to.x - from.x, to.y - from.y, 0.0);
    if d.length_squared() < 1e-6 {
        Vec3::Y
    } else {
        d.normalize()
    }
}

/// A point of the shot's frame (+Y `forward`, +Z up, about `origin`) in game space.
pub fn in_frame(origin: Vec3, forward: Vec3, p: Vec3) -> Vec3 {
    let right = forward.cross(Vec3::Z);
    origin + right * p.x + forward * p.y + Vec3::Z * p.z
}

/// The four sides of the line from the attacker to the target, level: right, left, back,
/// front (the order of [`CameraRoom`]).
pub fn sides(forward: Vec3) -> [Vec3; 4] {
    let right = forward.cross(Vec3::Z);
    [right, -right, -forward, forward]
}

/// What the camera's rays see: the world. No actor's capsule, hit volume or ragdoll (the
/// module's notes), nothing that moves, no trigger or zone.
fn ray_filter() -> fnv_phys::Filter {
    fnv_phys::Filter {
        exclude_layers: fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes()),
        no_dynamic: true,
        no_actors: true,
        ..Default::default()
    }
}

/// How far the way is free from `from` along `dir`, up to [`ROOM_REACH`].
fn free(phys: &mut PhysState, from: Vec3, dir: Vec3) -> f32 {
    phys.ray(from.to_array(), dir.to_array(), ROOM_REACH, ray_filter()).map_or(ROOM_REACH, |h| h.distance)
}

/// The room about the attacker and the target for the VATS area functions.
fn measure_room(phys: &mut PhysState, attacker: Vec3, target: Vec3) -> CameraRoom {
    let s = sides(level_forward(attacker, target));
    CameraRoom { area_free: s.map(|d| free(phys, attacker, d)), target_visible: s.map(|d| free(phys, target, d)) }
}

/// `want`, or short of it in front of what lies between `from` and it (game space).
fn clear(phys: &mut PhysState, from: Vec3, want: Vec3) -> Vec3 {
    let d = want - from;
    let len = d.length();
    if len < 1e-3 {
        return want;
    }
    match phys.ray(from.to_array(), (d / len).to_array(), len + WALL_MARGIN, ray_filter()) {
        Some(h) => from + d / len * (h.distance - WALL_MARGIN).clamp(0.0, len),
        None => want,
    }
}

/// The player's middle (game space): halfway from the feet to the eye.
fn player_middle(p: &Player) -> Vec3 {
    (Vec3::from_array(p.feet) + Vec3::from_array(p.eye())) * 0.5
}

/// Where a camera that looks at the player looks: this share of the way from the feet to
/// the eye, the upper chest [design: the masters' shots that look at the attacker stand at
/// about the height of its face (`playerfacecam01.nif`, `snipercamshoot02.nif`, 38 to 50
/// units above the middle) [observed]; aimed at the middle they showed the body from the
/// neck down, the fork owner's playtest build 2026-11-10].
pub const PLAYER_LOOK_HEIGHT: f32 = 0.85;

fn player_look(p: &Player) -> Vec3 {
    let (feet, eye) = (Vec3::from_array(p.feet), Vec3::from_array(p.eye()));
    feet + (eye - feet) * PLAYER_LOOK_HEIGHT
}

/// An actor's middle (game space), the player's for the player: its hit volume's aim
/// point, and for a corpse, which has none once its ragdoll is in the world
/// (`crate::hitvol`), its ragdoll's centre. Without the second a kill camera lost its
/// target a frame after the kill and stood about the attacker instead, the room about
/// the two measured as none [observed: the fork owner's playtest 2026-11-10, "the kill
/// camera still shows/rotates around the player character and not the target/killed
/// NPC"; the run's log, `room right/left/back/front 0/0/0/0`].
fn middle(phys: &PhysState, player: &Player, actor: FormId) -> Option<Vec3> {
    if actor == FormId::PLAYER_REF {
        return Some(player_middle(player));
    }
    phys.world.actor_aim_point(actor.0).or_else(|| phys.world.ragdoll_center(actor.0)).map(Vec3::from_array)
}

/// Watch the last mark, start and run the kill camera.
#[allow(clippy::too_many_arguments)]
fn update(
    (real, virt): (Res<Time<Real>>, Res<Time<Virtual>>),
    state: Res<State<PlayState>>,
    vats: Res<VatsMode>,
    mut cam: ResMut<KillCam>,
    mut script: Option<ResMut<ScriptState>>,
    mut phys: ResMut<PhysState>,
    player: Res<Player>,
    mut control: ResMut<PlayerControl>,
    mut models: ModelCtx,
    setting: Option<Res<KillCamSetting>>,
) {
    let setting = setting.map_or_else(KillCamSetting::default, |s| *s);
    let cam = &mut *cam;
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let playing = *state.get() == PlayState::Playing;
    let player_dead = rt.game.is_dead(FormId::PLAYER_REF);
    // No kill camera has the view: none, or an attack's, which a kill's takes over. (The
    // last attack leaves a moment into its own camera, so a near target dies while that
    // still plays; waiting for it to end, the target was dead before it was watched and
    // no kill camera came [observed: a run of 2026-11-10, a dog killed by the last of two
    // marks 0.07 s after the shot, no `kill camera:` line].)
    let free = !matches!(cam.show.as_ref().map(|s| s.kind), Some(Kind::Kill));
    // The last mark: watched from the frame its attack starts.
    if let Some((shot, _)) = vats.final_shot(&player) {
        let same = cam.watch.as_ref().is_some_and(|w| w.shot.target == shot.target);
        if !same && free && !rt.game.is_dead(shot.target) {
            cam.watch = Some(Watch { shot, since_off: 0.0 });
        } else if let Some(w) = cam.watch.as_mut().filter(|_| same) {
            w.shot = shot;
        }
    } else if let Some(w) = cam.watch.as_mut() {
        if vats.phase == Phase::Off {
            w.since_off += virt.delta_secs();
        }
        if w.since_off > WATCH_AFTER || vats.phase == Phase::Marking {
            cam.watch = None;
        }
    }
    // A kill: whether the setting and the fight leave room for a camera.
    let mut killed = cam.watch.as_ref().filter(|w| free && rt.game.is_dead(w.shot.target)).map(|w| w.shot);
    if let Some(shot) = killed {
        cam.watch = None;
        let others = enemies_in_combat(&rt.game, shot.target);
        let skip = match setting {
            KillCamSetting::Off => {
                info!("kill camera: off in the settings");
                cam.off += 1;
                true
            }
            KillCamSetting::LastEnemy if !others.is_empty() => {
                let names: Vec<String> = others.iter().map(|o| format!("{} {o}", rt.game.data.name(*o))).collect();
                info!("kill camera: skipped, {} enemies still in combat ({})", others.len(), names.join(", "));
                cam.skipped += 1;
                true
            }
            _ => false,
        };
        if skip {
            killed = None;
        }
    }
    // Choose the path.
    if let Some(shot) = killed {
        let name = rt.game.data.name(shot.target);
        let attacker = player_middle(&player);
        let room = match middle(&phys, &player, shot.target) {
            Some(t) => measure_room(&mut phys, attacker, t),
            None => CameraRoom::default(),
        };
        let outcome = rt.game.kill_outcome(shot.target);
        let path = rt.game.pick_camera_path(VatsShot { success: true, ..shot }, outcome, room);
        let shots = path.and_then(|p| rt.game.data.camera_paths.get(&p)).map(|p| camera_paths::hit_shots(p, &rt.game.data.camera_shots)).unwrap_or_default();
        match path.filter(|_| !shots.is_empty()) {
            Some(p) => {
                let names: Vec<String> = shots.iter().map(|s| rt.game.data.name(*s)).collect();
                info!(
                    "kill camera: {name} {} killed by the last mark: path {} ({p}), shots {}; room right/left/back/front {:.0}/{:.0}/{:.0}/{:.0}, target seen {:.0}/{:.0}/{:.0}/{:.0}{}",
                    shot.target,
                    rt.game.data.name(p),
                    names.join(", "),
                    room.area_free[0],
                    room.area_free[1],
                    room.area_free[2],
                    room.area_free[3],
                    room.target_visible[0],
                    room.target_visible[1],
                    room.target_visible[2],
                    room.target_visible[3],
                    if outcome.dismember || outcome.explode { ", a part is gone" } else { "" }
                );
                // An attack's camera still playing gives way to it.
                if let Some(m) = cam.show.take().and_then(|s| s.playing).and_then(|p| p.imod) {
                    rt.game.remove_image_modifier(m);
                }
                cam.linger = None;
                cam.show = Some(Show { kind: Kind::Kill, target: shot.target, shots, next: 0, playing: None, real: 0.0, held: 0, angle: None, fallback: None, seen: None });
                cam.shown += 1;
            }
            None => info!("kill camera: {} killed by the last mark: no camera path chosen", shot.target),
        }
    }
    // A classic attack's camera (module notes): asked once per stage by its serial.
    if let Some((serial, ask)) = vats.camera_ask().filter(|(serial, _)| *serial != cam.asked) {
        cam.asked = serial;
        if cam.show.is_none() && playing && !player_dead {
            let attacker = player_middle(&player);
            let room = match middle(&phys, &player, ask.shot.target) {
                Some(t) => measure_room(&mut phys, attacker, t),
                None => CameraRoom::default(),
            };
            let action = match ask.stage {
                CamStage::Shoot => ShotAction::Shoot,
                CamStage::Hit => ShotAction::Hit,
            };
            let path = rt.game.pick_camera_path(ask.shot, fnv_game::vats::VatsOutcome::default(), room);
            let shots = path.and_then(|p| rt.game.data.camera_paths.get(&p)).map(|p| camera_paths::action_shots(p, &rt.game.data.camera_shots, action)).unwrap_or_default();
            // An attack is always shown as it leaves; what follows it only where the path
            // has a shot for that.
            let fallback = (shots.is_empty() && ask.stage == CamStage::Shoot).then(own_shot);
            if shots.is_empty() && fallback.is_none() {
                debug!("vats camera: attack {} on {}, {:?}: no path with such a shot", ask.mark + 1, ask.shot.target, ask.stage);
            } else {
                // The angle (the module's notes): the next one for a stay's first attack
                // and once this one has had its time.
                let target = middle(&phys, &player, ask.shot.target).unwrap_or(attacker);
                let first = ask.mark == 0 && ask.stage == CamStage::Shoot;
                if first || cam.angle.is_none() || (ask.stage == CamStage::Shoot && cam.angle_for >= ANGLE_DWELL) {
                    let at = cam.angle.map_or(0, |i| (i + 1) % Angle::CYCLE.len());
                    (cam.angle, cam.angle_for) = (Some(at), 0.0);
                    cam.angle_side = roomier_side(&mut phys, Angle::CYCLE[at], attacker, player_look(&player), target);
                }
                let (angle, side) = (Angle::CYCLE[cam.angle.unwrap_or(0)], cam.angle_side);
                let names: Vec<String> = shots.iter().map(|s| rt.game.data.name(*s)).collect();
                info!(
                    "vats camera: attack {} on {} {}, {:?}: angle {}{}; {}",
                    ask.mark + 1,
                    rt.game.data.name(ask.shot.target),
                    ask.shot.target,
                    ask.stage,
                    angle.name(),
                    match (angle, side < 0.0) {
                        (Angle::FirstPerson, _) => "",
                        (_, true) => ", from the left",
                        (_, false) => ", from the right",
                    },
                    match path.filter(|_| !shots.is_empty()) {
                        Some(p) => format!("path {} ({p}), shots {}", rt.game.data.name(p), names.join(", ")),
                        None => "no path with such a shot, NVForever's own clocks".to_string(),
                    }
                );
                cam.show = Some(Show { kind: Kind::Attack(ask.stage), target: ask.shot.target, shots, next: 0, playing: None, real: 0.0, held: 0, angle: Some((angle, side)), fallback, seen: None });
                cam.attacks += 1;
            }
        }
    }
    // Between two attacks' cameras the last angle holds the view.
    if cam.show.is_some() {
        cam.linger = None;
    } else if let Some(l) = cam.linger.as_mut() {
        l.left -= real.delta_secs();
        if l.left <= 0.0 || vats.phase != Phase::Firing || !playing {
            cam.linger = None;
        }
    }
    let Some(show) = cam.show.as_mut() else {
        control.cinematic = cam.linger.is_some_and(|l| l.angle != Angle::FirstPerson);
        return;
    };
    // The place the camera is held about and looks at follows the target smoothly: a
    // corpse's place is its ragdoll's, which moves a physics step at a time while the
    // picture is drawn every frame, and a camera hung on it shook with every step
    // [observed: the fork owner's playtest 2026-11-10, "the kill camera jitters a lot"].
    if let Some(at) = middle(&phys, &player, show.target) {
        show.seen = Some(match show.seen {
            Some(was) if was.distance(at) <= FOLLOW_JUMP => was.lerp(at, 1.0 - (-real.delta_secs().min(0.25) / FOLLOW_SECONDS).exp()),
            _ => at,
        });
    }
    // Cut short.
    let stop = if player_dead {
        Some("the player died")
    } else if !playing {
        Some("a menu opened")
    } else if matches!(show.kind, Kind::Attack(_)) && vats.phase != Phase::Firing {
        Some("the attacks stopped")
    } else {
        None
    };
    let label = show.kind.label();
    let dt = if virt.is_paused() { 0.0 } else { real.delta_secs() };
    show.real += dt;
    if show.angle.is_some() {
        cam.angle_for += dt;
    }
    // An attack's last Shoot shot shows the attack: it stays until that is out.
    let shows_attack = show.kind == Kind::Attack(CamStage::Shoot) && show.next >= show.shots.len();
    let finished = match (&mut show.playing, stop) {
        (_, Some(_)) => true,
        (Some(p), None) if shows_attack => {
            p.real += dt;
            if vats.attack_out() {
                p.out_for += dt;
            }
            (p.real >= p.length && p.out_for >= SHOOT_AFTER) || p.real >= SHOOT_PATIENCE
        }
        (Some(p), None) => {
            p.real += dt;
            p.real >= p.length
        }
        (None, None) => true,
    };
    if finished {
        if let Some(p) = show.playing.take() {
            if let Some(m) = p.imod {
                rt.game.remove_image_modifier(m);
            }
        }
        let next = show.shots.get(show.next).copied().filter(|_| stop.is_none());
        // A shot a script changed (SetCameraShot…, `nvse_jg5`) plays as changed.
        let next = next.and_then(|s| fnv_game::nvse_jg5::camera_shot(&rt.game, s).map(|d| (Some(s), d.clone()))).or_else(|| show.fallback.take().filter(|_| stop.is_none()).map(|d| (None, d)));
        match next {
            Some((s, def)) => {
                show.next += 1;
                // An attack's camera stands at its angle, not where the shot's model does.
                let model = def.model.as_deref().filter(|_| !def.has(FIRST_PERSON_CAMERA) && show.angle.is_none()).and_then(|m| models.camera_model(&format!("meshes\\{m}")));
                let attacker = player_middle(&player);
                let target = show.seen.unwrap_or(attacker);
                let forward = level_forward(attacker, target);
                let origin = match def.location {
                    ShotActor::Target => target,
                    ShotActor::Attacker | ShotActor::Projectile => attacker,
                };
                let look = look_point(&def, player_look(&player), target);
                let first_turn = model.as_ref().map_or(Quat::IDENTITY, |m| m.root_at(0.0).1);
                let imod = def.imod.filter(|_| show.angle.is_none());
                if let Some(m) = imod {
                    rt.game.apply_image_modifier(m);
                }
                let length = match show.kind {
                    Kind::Kill => shot_length(&def),
                    Kind::Attack(stage) => attack_length(&def, stage),
                };
                // The classic style holds the last shot of a hit's camera and of the kill
                // camera longer ([`CLASSIC_HOLD`]), the kill camera's longer still
                // ([`KILL_HOLD`]).
                let last = show.next >= show.shots.len();
                let length = match show.kind {
                    Kind::Kill if last => length + CLASSIC_HOLD + KILL_HOLD,
                    Kind::Attack(CamStage::Hit) if last => length + CLASSIC_HOLD,
                    _ => length,
                };
                let world = if def.time_global > 0.0 { def.time_global.clamp(SLOWEST, 1.0) } else { 1.0 };
                let from = model.as_ref().map_or(Vec3::ZERO, |m| m.root_at(0.0).0);
                info!(
                    "{label}: shot {} of {} {}: {}, held by the {:?} from {:.0}/{:.0}/{:.0} (right/ahead/up), looking at the {:?}, {length:.1} s at world speed {world:.2}, the player at {:.2} of real time{}",
                    show.next,
                    show.shots.len().max(1),
                    s.map_or_else(|| "NVForever's own".to_string(), |s| rt.game.data.name(s)),
                    match (&model, def.has(FIRST_PERSON_CAMERA), show.angle) {
                        (_, _, Some((angle, _))) => format!("angle {}", angle.name()),
                        (_, true, _) => "first person".to_string(),
                        (Some(_), _, _) => def.model.clone().unwrap_or_default(),
                        (None, _, _) => format!("{} not loaded, the view stays", def.model.clone().unwrap_or_default()),
                    },
                    def.location,
                    from.x,
                    from.y,
                    from.z,
                    def.target,
                    world * player_scale_of(def.time_player, world),
                    imod.map_or_else(String::new, |m| format!(", image space modifier {}", rt.game.data.name(m))),
                );
                show.playing = Some(Playing { def, model, real: 0.0, length, out_for: 0.0, origin, forward, look, first_turn, imod });
            }
            None => {
                info!("{label}: done ({:.1} s{}), input held {} frames", show.real, stop.map_or_else(String::new, |w| format!("; {w}")), show.held);
                let linger = show.angle.filter(|_| stop.is_none()).map(|(angle, side)| Linger { angle, side, target: show.target, left: ANGLE_LINGER });
                cam.show = None;
                cam.linger = linger;
            }
        }
    }
    control.cinematic = match cam.show.as_ref() {
        Some(s) => s.playing.as_ref().is_some_and(|p| match s.angle {
            Some((angle, _)) => angle != Angle::FirstPerson,
            None => p.model.is_some(),
        }),
        None => cam.linger.is_some_and(|l| l.angle != Angle::FirstPerson),
    };
}

/// Where a shot looks (game space): its Target actor's point (`attacker`, `target`: the
/// target's middle, the player's upper chest, [`PLAYER_LOOK_HEIGHT`]), moved Target %
/// Between Actors of the way to the other actor's.
pub fn look_point(def: &CameraShotDef, attacker: Vec3, target: Vec3) -> Vec3 {
    let (base, other) = match def.target {
        ShotActor::Target => (target, attacker),
        ShotActor::Attacker | ShotActor::Projectile => (attacker, target),
    };
    base + (other - base) * (def.target_pct / 100.0).clamp(0.0, 1.0)
}

/// Put the camera where the playing shot holds it.
fn place(
    cam: Res<KillCam>,
    player: Res<Player>,
    mut phys: ResMut<PhysState>,
    mut cams: Query<&mut Transform, With<PlayerCamera>>,
) {
    let Ok(mut t) = cams.single_mut() else { return };
    let attacker = player_middle(&player);
    // An attack's camera at its angle, and the angle that holds between two of them.
    let angled = match cam.show.as_ref() {
        Some(show) => show.angle.filter(|_| show.playing.is_some()).map(|(angle, side)| (angle, side, show.target, show.seen)),
        None => cam.linger.map(|l| (l.angle, l.side, l.target, None)),
    };
    if let Some((angle, side, who, seen)) = angled {
        let target = seen.or_else(|| middle(&phys, &player, who)).unwrap_or(attacker);
        let Some((want, look)) = angle_spot(angle, side, attacker, player_look(&player), target) else { return };
        let eye = clear(&mut phys, look, want);
        let (from, to) = (space::to_bevy(eye.to_array()), space::to_bevy(look.to_array()));
        if (to - from).length_squared() > 1e-4 && from.is_finite() && to.is_finite() {
            *t = Transform::from_translation(from).looking_at(to, Vec3::Y);
        }
        return;
    }
    let Some(show) = cam.show.as_ref() else { return };
    let Some(p) = show.playing.as_ref() else { return };
    let Some(model) = p.model.as_ref() else { return };
    let target = show.seen.or_else(|| middle(&phys, &player, show.target)).unwrap_or(attacker);
    let (origin, forward) = if p.def.has(POSITION_FOLLOWS_LOCATION) {
        let o = match p.def.location {
            ShotActor::Target => target,
            ShotActor::Attacker | ShotActor::Projectile => attacker,
        };
        (o, p.forward)
    } else {
        (p.origin, p.forward)
    };
    let look = if p.def.has(ROTATION_FOLLOWS_TARGET) { look_point(&p.def, player_look(&player), target) } else { p.look };
    let (at, turn) = model.root_at(p.real);
    let want = in_frame(origin, forward, at);
    let eye = clear(&mut phys, look, want);
    // The model's sway: its turn since its first key, about the frame's axes.
    let sway = turn * p.first_turn.inverse();
    let (axis, angle) = sway.to_axis_angle();
    let frame_axis = in_frame(Vec3::ZERO, forward, axis);
    let sway_bevy = if angle.abs() > 1e-5 && frame_axis.length_squared() > 1e-8 { Quat::from_axis_angle(space::to_bevy(frame_axis.to_array()).normalize(), angle) } else { Quat::IDENTITY };
    let from = space::to_bevy(eye.to_array());
    let to = space::to_bevy(look.to_array());
    if (to - from).length_squared() < 1e-4 {
        return;
    }
    let base = Transform::from_translation(from).looking_at(to, Vec3::Y);
    let out = Transform { translation: from, rotation: (sway_bevy * base.rotation).normalize(), scale: Vec3::ONE };
    if out.translation.is_finite() && out.rotation.is_finite() {
        *t = out;
    }
}

/// Strike from what the frame asked of the character (the keys, held already by
/// `bindings::ControlInput`, and the test aids that stand in for them) what a playing
/// kill camera holds (module notes): moving, jumping, the trigger, aiming, reload and
/// ammo swap. After `control::enforce_locks`, before anything acts on it.
pub fn hold_input(mut cam: ResMut<KillCam>, mut intent: ResMut<MoveIntent>, mut control: ResMut<PlayerControl>) {
    let Some(show) = cam.show.as_mut() else { return };
    let (mut i, mut c) = (*intent, *control);
    (i.forward, i.right, i.jump) = (0.0, 0.0, false);
    (c.trigger, c.aiming, c.reload, c.ammo_swap, c.ready_held) = (false, false, false, false, None);
    if (i, c) != (*intent, *control) {
        show.held += 1;
    }
    intent.set_if_neq(i);
    control.set_if_neq(c);
}

fn report(mut exit: MessageReader<AppExit>, cam: Res<KillCam>) {
    if exit.read().last().is_none() {
        return;
    }
    info!("kill camera summary: {} shown, {} skipped, {} off", cam.shown, cam.skipped, cam.off);
    if cam.attacks > 0 {
        info!("vats camera summary: {} attack cameras shown", cam.attacks);
    }
}

pub struct KillCamPlugin;

impl Plugin for KillCamPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KillCam>()
            .init_resource::<KillCamSetting>()
            .add_systems(Update, update.in_set(crate::PlaySet::Activation).after(crate::vats_mode::VatsUpdate).run_if(resource_exists::<PhysState>))
            .add_systems(Update, hold_input.after(crate::control::enforce_locks).after(crate::PlaySet::Input).before(crate::PlaySet::Physics))
            .add_systems(PostUpdate, place.before(bevy::transform::TransformSystems::Propagate).run_if(resource_exists::<PhysState>))
            .add_systems(Last, report);
        crate::settings::add_choice_list(
            app,
            crate::settings::Setting {
                key: "kill-camera",
                group: crate::pause::GROUP_MENUS,
                label: "Kill camera",
                note: "When a VATS shot kills its last mark, a camera shows the kill. Last enemy: only when no other enemy still fights you. Every kill: always. Off: never.",
                switches: "",
                get: |_| false,
                set: |_, _| {},
                slider: None,
            },
            crate::settings::ChoiceList {
                names: |_| KillCamSetting::ALL.iter().map(|s| s.name().to_string()).collect(),
                chosen: |w| w.resource::<KillCamSetting>().name().to_string(),
                pick: setting_pick,
                save: |w| w.resource::<KillCamSetting>().name().to_string(),
                load: setting_pick,
                ask_keep: false,
            },
        );
    }
}

#[cfg(test)]
mod angle_tests {
    use super::*;

    #[test]
    fn the_angles_stand_where_they_show_whom_they_name() {
        // The attacker at the origin, the target 600 ahead (+Y), their middles 60 up.
        let (a, chest, t) = (Vec3::new(0.0, 0.0, 60.0), Vec3::new(0.0, 0.0, 100.0), Vec3::new(0.0, 600.0, 60.0));
        assert_eq!(angle_spot(Angle::FirstPerson, 1.0, a, chest, t), None, "the eye's own view");
        // Both: behind the attacker and to its right, looking on toward the target.
        let (at, look) = angle_spot(Angle::Both, 1.0, a, chest, t).unwrap();
        assert!(at.y < a.y && at.x > 0.0 && look.y > 400.0, "{at:?} {look:?}");
        assert_eq!(angle_spot(Angle::Both, -1.0, a, chest, t).unwrap().0.x, -at.x, "the left side is its mirror");
        // The target alone: between the two, nearer the target; the attacker is behind it.
        let (at, look) = angle_spot(Angle::Target, 1.0, a, chest, t).unwrap();
        assert_eq!(look, t);
        assert!(at.y > 300.0 && at.y < t.y, "{at:?}");
        // A target at arm's length leaves no room between the two: from beside them.
        let (at, _) = angle_spot(Angle::Target, 1.0, a, chest, Vec3::new(0.0, 100.0, 60.0)).unwrap();
        assert!(at.x > 90.0, "{at:?}");
    }

    #[test]
    fn every_angle_has_its_turn() {
        for angle in [Angle::Both, Angle::FirstPerson, Angle::Target] {
            assert!(Angle::CYCLE.contains(&angle), "{}", angle.name());
        }
        assert_eq!(own_shot().action, ShotAction::Shoot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_game::camera_paths::ShotAction;

    fn def(location: ShotActor, target: ShotActor, pct: f32) -> CameraShotDef {
        CameraShotDef {
            model: None,
            action: ShotAction::Hit,
            location,
            target,
            flags: 7,
            time_player: 4.0,
            time_target: 3.0,
            time_global: 0.25,
            max_time: 3.0,
            min_time: 1.0,
            target_pct: pct,
            imod: None,
        }
    }

    #[test]
    fn the_frame_faces_from_the_attacker_to_the_target() {
        let (a, t) = (Vec3::new(0.0, 0.0, 60.0), Vec3::new(1000.0, 0.0, 70.0));
        let f = level_forward(a, t);
        assert!((f - Vec3::X).length() < 1e-6, "{f}");
        // A front shot's camera (−59, −226, 18) about the target stands between it and
        // the attacker, a little to the left of the line.
        let cam = in_frame(t, f, Vec3::new(-59.0, -226.0, 18.0));
        assert!((cam - Vec3::new(774.0, 59.0, 88.0)).length() < 1e-3, "{cam}");
        let [right, left, back, front] = sides(f);
        assert!((right - Vec3::new(0.0, -1.0, 0.0)).length() < 1e-6 && (left + right).length() < 1e-6);
        assert!((back + Vec3::X).length() < 1e-6 && (front - Vec3::X).length() < 1e-6);
        // Standing one over the other: +Y.
        assert_eq!(level_forward(a, a + Vec3::Z * 5.0), Vec3::Y);
    }

    #[test]
    fn the_look_point_moves_towards_the_other_actor() {
        let (a, t) = (Vec3::ZERO, Vec3::new(100.0, 0.0, 0.0));
        assert_eq!(look_point(&def(ShotActor::Target, ShotActor::Target, 0.0), a, t), t);
        assert_eq!(look_point(&def(ShotActor::Target, ShotActor::Target, 50.0), a, t), Vec3::new(50.0, 0.0, 0.0));
        assert_eq!(look_point(&def(ShotActor::Attacker, ShotActor::Attacker, 0.0), a, t), a);
    }

    #[test]
    fn kill_camera_settings_are_known_by_their_names() {
        assert_eq!(KillCamSetting::default(), KillCamSetting::LastEnemy);
        for s in KillCamSetting::ALL {
            assert_eq!(KillCamSetting::by_name(s.name()), Some(s));
        }
        assert_eq!(KillCamSetting::by_name(" every KILL "), Some(KillCamSetting::EveryKill));
        assert_eq!(KillCamSetting::by_name("On"), None);
    }

    #[test]
    fn a_playing_kill_camera_holds_the_frame_s_requests() {
        let mut app = App::new();
        app.init_resource::<KillCam>().init_resource::<MoveIntent>().init_resource::<PlayerControl>().add_systems(Update, hold_input);
        let ask = |app: &mut App| {
            *app.world_mut().resource_mut::<MoveIntent>() = MoveIntent { forward: 1.0, right: -1.0, jump: true, ..Default::default() };
            let mut c = app.world_mut().resource_mut::<PlayerControl>();
            (c.weapon_out, c.trigger, c.aiming, c.reload) = (true, true, true, true);
        };
        // No camera: nothing struck.
        ask(&mut app);
        app.update();
        assert_eq!(app.world().resource::<MoveIntent>().forward, 1.0);
        assert!(app.world().resource::<PlayerControl>().trigger);
        app.world_mut().resource_mut::<KillCam>().show = Some(Show { kind: Kind::Kill, target: FormId(0x1234), shots: Vec::new(), next: 0, playing: None, real: 0.0, held: 0, angle: None, fallback: None, seen: None });
        ask(&mut app);
        app.update();
        let (i, c) = (*app.world().resource::<MoveIntent>(), *app.world().resource::<PlayerControl>());
        assert_eq!((i.forward, i.right, i.jump), (0.0, 0.0, false));
        assert!(!c.trigger && !c.aiming && !c.reload && c.weapon_out, "the weapon stays out");
        // A frame that asks nothing is not counted.
        app.update();
        assert_eq!(app.world().resource::<KillCam>().show.as_ref().map(|s| s.held), Some(1));
    }

    #[test]
    fn a_shot_plays_its_max_time_within_bounds() {
        assert_eq!(shot_length(&def(ShotActor::Target, ShotActor::Target, 0.0)), 3.0);
        let mut d = def(ShotActor::Target, ShotActor::Target, 0.0);
        d.max_time = 0.01;
        d.min_time = 0.0;
        assert_eq!(shot_length(&d), SHOT_LEAST);
        d.max_time = 60.0;
        assert_eq!(shot_length(&d), SHOT_MOST);
    }
}
