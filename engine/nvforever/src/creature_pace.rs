//! How fast creatures move, and the stance they move in (docs/spec-notes/actors.md §8.10;
//! the owner's playtest 2026-10-16: cazadors moved far too slowly and their walk stopped
//! and started).
//!
//! - **Speed**: a creature walks at the root motion of its `Forward` clip and runs at
//!   that of its `FastForward` clip, times its scale, its Speed Mult (CREA ACBS) and
//!   its skeleton's share ([`skeleton_share`]: 1 but for cazadors). The AI
//!   moves it at those speeds (`fnv_ai::Ai::speeds`).
//!   People walk and run at `fnv_ai::NPC_WALK` and `NPC_RUN` times their Speed Mult and
//!   their body armour's share ([`person_pace`]), the armour looked at again every
//!   second. The Speed Mult is the current one ([`current_speed_mult`]: SetAV, ModAV,
//!   effects and damage count), read again every quarter second for people and
//!   creatures alike, and the speeds are worked out afresh from the unscaled ones when it
//!   changes (issue #2118; [`refresh_speeds`]). Before this every actor
//!   walked at 86.8 and ran at 344 units/s: a cazador's walk clip (217 units/s) played
//!   at its slowest rate (0.4), and 344 is the point between its walk and run clips
//!   (the geometric mean of 217 and 546), so a small slow-down swapped the two clips back
//!   and forth: the stop-and-start.
//! - **Clip rate**: the clips play `Speed Mult × the share` faster
//!   (`Locomotion::pace`), as the movement comes from them, and the move is chosen for the
//!   velocity divided by that: a medium cazador walking at 379 units/s plays its walk clip
//!   at 1.75, not its run clip at 0.7.
//! - **Turning**: a creature turns at its CREA `TNAM` × [`TNAM_TURN_MULT`] degrees a
//!   second, walking and standing, whatever its scale ([`creature_turn_rate`];
//!   [`CREATURE_TURN_RATE`] × its scale when TNAM is 0), and turns on the spot only when
//!   its way is more than 60 degrees off; the people's 120 degrees a second and 20
//!   degrees made a fast cazador stop at the start of every short walk.
//! - **Stance**: a creature whose folder has `h2h` movement clips (the cazador, the dog,
//!   the mirelurk king) moves on them while it is not in the Normal detection state
//!   (Alert, Combat, Lost: `locomotion::Locomotion::set_alert`), on its `mt` clips
//!   otherwise; the cazador scurries along the ground while calm and flies when alert
//!   [observed: the owner, 2026-10-16]. Its speeds follow the stance's clips. The
//!   sonic-bark cyberdog, in the dog's folder, keeps its `mt` clips ([`has_alert_stance`]).
//!
//! Speeds are given once the creature is drawn (its clips are then loaded), so one walking
//! off-screen before that moves at the people's speeds; an actor no longer drawn keeps
//! the speeds it was last given until it is drawn again, when it is given its current
//! ones, Speed Mult 100 included ([`first_pace`]).

use crate::limbs::ActorRig;
use crate::locomotion::{ClipSet, Locomotion};
use crate::residents::Residents;
use bevy::prelude::*;
use fnv_actor::locomotion::Move;
use fnv_ai::{ActorSpeeds, Speeds};
use fnv_game::awareness::State;
use fnv_sim::FormId;
use nvf_play::script::{with_world, ScriptState};
use nvf_play::sim::SimState;

/// The share of the clip pace (root motion × scale × Speed Mult) a creature keeps when
/// its skeleton has none of its own ([`skeleton_share`]): the whole of it, walking and
/// running [observed: the AI observer in the original game, 2026-10-25, campaign
/// `creature_runs` on the Searchlight Airport runway (docs/spec-notes/actors.md §8.10),
/// steady stretches of 2–4 s; charges on the run clip of the stance they take in combat]:
/// a dog (scale 0.8) charged at 363 against its `h2hfastforward` 454.75 × 0.8 = 363.8, a
/// coyote (Speed Mult 110) 500 against 500.2, a radscorpion (0.6, 150) 281 against 283.5,
/// a bark scorpion (0.35, 250) 275 against 275.7, a feral ghoul 388 against 384, deathclaws
/// about 600 against 600 and 450 against 600 × 0.75; walks: radscorpions 67.0 against
/// 67.4, feral ghouls 97 against 95.7, deathclaws 119–121 against 120, mole rats 42.4
/// against 42.7, giant rats (1.1) 61.5 against 61.6, brahmin (1.25) 53.4 against 55.2;
/// a fleeing brahmin up to 234 against 222 (accelerating, 1.8 s). One per skeleton of the
/// base game and add-ons the same way (campaign `creature_runs_all`, actors.md §8.10):
/// within 6 % but for the protectron, among them every other flyer (eyebots, Mister
/// Gutsy, bloatflies) and robots, ants, centaurs, nightstalkers, spore carriers and
/// tunnelers, yao guai, roboscorpions, trogs, holograms and ghost people.
pub const CREATURE_SPEED_FIT: f32 = 1.0;

/// A skeleton's shares of its clips' pace, walking and running ([`clip_speeds`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Share {
    pub walk: f32,
    pub run: f32,
}

/// A skeleton's own shares of its clips' pace ([`CREATURE_SPEED_FIT`] for every other)
/// [observed: the AI observer in the original game]:
/// - cazadors (`nvcazadores`) 0.70 walking and running (2026-10-16: young ones at the
///   Tribal Village walked 228 against a clip pace of 325, medium ones 380 against 542;
///   charging, 576 and 974 against 818 and 1364). Why is open: no game setting gives 0.70,
///   every other flyer keeps the whole pace, and on the airport runway (2026-10-25,
///   `creature_runs_all`, `creature_flee`) a young cazador at Aggression 0 flew at
///   440–443 in three sessions, its `h2hforward` 298.15 × 0.5 × 3.0 = 447 whole, and dashed
///   at 680–1,090 for under a second;
/// - protectrons 0.88 walking, 1 running (2026-10-25: VCrTier0CrProtectron walked 22.0,
///   22.2 and 22.6 in three sessions against its `mtforward` 25.25, and ran a mean of 84.4
///   over 17 s against its `mtfastforward` 83.37).
///
/// The shares fitted before the airport runs (brahmin 1.4, mole rats 0.80, giant rats 0.76,
/// deathclaws and feral ghouls 0.70) came from a room without navmesh, a pack brahmin
/// keeping step with its merchant and a mole rat charging uphill (actors.md §8.10).
pub fn skeleton_share(skeleton: &str) -> Share {
    let path = fnv_vfs::normalize(skeleton);
    let whole = Share { walk: CREATURE_SPEED_FIT, run: CREATURE_SPEED_FIT };
    if path.contains("creatures\\nvcazadores\\") {
        Share { walk: 0.70, run: 0.70 }
    } else if path.contains("creatures\\protectron\\") {
        Share { walk: 0.88, ..whole }
    } else {
        whole
    }
}

/// Whether a creature of `skeleton` takes its folder's alert stance (`h2h` movement clips,
/// [`stance`]): every one but the sonic-bark cyberdog (`creatures\dog\skeleton_sonicbark.nif`)
/// [observed: the AI observer in the original game, 2026-10-25, campaign
/// `creature_runs_all`: NVDLC03CrCyberDogMilitary01Ranged (03015800, scale 1, Speed Mult
/// 100, a ranged attacker) charged at 333.1 on the `mt` run (`mtfastforward` 333.07), not
/// the dog folder's `h2hfastforward` 454.75, on which the plain dog charges (363 at scale
/// 0.8); hypothesis: that a creature attacking with a ranged weapon keeps its `mt` clips
/// when alert, not measured on any other skeleton]. Without it the cyberdog keeps its calm
/// clips, and its calm limp, in combat.
pub fn has_alert_stance(skeleton: &str) -> bool {
    !fnv_vfs::normalize(skeleton).ends_with("creatures\\dog\\skeleton_sonicbark.nif")
}

/// What a creature's CREA `TNAM` "Turning Speed" is multiplied by for its turn rate in
/// degrees per second, walking and standing [observed 2026-10-30: the AI observer in the
/// original game, campaign `o4turn` on the Searchlight Airport runway (issue #915,
/// actors.md §8.10 "TNAM"), headings every game loop: 35 of 38 steady turns of 13
/// creatures were TNAM × 1.25, × 3 or × 0.075 to within 1%, the base scale playing no
/// part (VCrCazadorTier1TypeA, scale 0.8, TNAM 200: 250.0 and 600.1); × 1.25 is the one
/// every creature showed on big turns, walking or standing (CrMoleRat 100: 125.0,
/// NVCrCazador 720: 900.2, CrDeathClaw 75: 93.7 five times)]. × 3, also on big turns
/// towards a target, is left out until what picks it is known (observer-checks 36), and
/// × 0.075 (aim tracking within about 6 degrees) is not a turn the walker makes [design].
pub const TNAM_TURN_MULT: f32 = 1.25;

/// A creature's turn rate at scale 1, degrees per second, when its TNAM is 0 (templated
/// records whose stats come from elsewhere carry 0; a mod's creature may too): it scales
/// with the creature's size [observed: medium cazadors (scale 1) turned at up to 304
/// degrees a second, young ones (scale 0.5) at up to 137, headings 0.16 s apart;
/// hypothesis: the rule; the fit came before TNAM was measured, [`TNAM_TURN_MULT`]].
pub const CREATURE_TURN_RATE: f32 = 300.0;

/// A creature's turn rate, degrees per second, walking and standing: its `tnam` (CREA
/// TNAM through the template chain, `Game::creature_turning_speed`) × [`TNAM_TURN_MULT`],
/// its `scale` playing no part; [`CREATURE_TURN_RATE`] × `scale` without a TNAM above 0.
pub fn creature_turn_rate(tnam: Option<f32>, scale: f32) -> f32 {
    match tnam.filter(|t| t.is_finite() && *t > 0.0) {
        Some(t) => t * TNAM_TURN_MULT,
        None => CREATURE_TURN_RATE * scale,
    }
}

/// How far off its way a standing creature may be before it turns on the spot instead
/// of turning as it sets off, radians [design: the cazadors made 85-89% of their fast
/// turns while moving].
pub const CREATURE_SPOT_TURN_START: f32 = 60.0 * std::f32::consts::PI / 180.0;

/// Changes between walk and run clips logged, the first ones only.
const SWAPS_LOGGED: usize = 20;

pub struct CreaturePacePlugin;

impl Plugin for CreaturePacePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaceStats>()
            .add_systems(
                Update,
                (
                    give_speeds.after(nvf_play::PlaySet::Scripts).before(crate::residents::drive),
                    refresh_speeds.after(give_speeds).before(crate::residents::drive),
                    stance.after(nvf_play::PlaySet::Scripts).before(crate::locomotion::walk_test),
                ),
            )
            .add_systems(Last, report);
    }
}

/// What a drawn actor's speeds were last worked out from ([`give_speeds`]), to see a
/// change ([`refresh_speeds`]).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
struct Paced {
    /// Its current Speed Mult, percent ([`current_speed_mult`]).
    mult: f32,
    kind: PaceKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PaceKind {
    /// A person: the people's speeds × its Speed Mult × its body armour's share
    /// ([`person_pace`]).
    Person { armour: f32 },
    /// A creature: its clips × its scale × its Speed Mult × its skeleton's share, turning
    /// at `turn` degrees a second ([`creature_turn_rate`]; `tnam` for the log).
    Creature { scale: f32, share: Share, turn: f32, tnam: Option<f32> },
    /// The player (its own speeds, `nvf_play::script_input`) or an actor without a record:
    /// left at the AI's speeds.
    Own,
}

/// Marks a paced creature (for [`stance`]).
#[derive(Component)]
struct PacedCreature;

#[derive(Resource, Default)]
struct PaceStats {
    paced: usize,
    /// People whose Speed Mult is not 100 (among `paced`).
    npcs: usize,
    /// Speeds given to people for their armour.
    people: usize,
    /// Speeds given again for a new current Speed Mult (people and creatures).
    repaced: usize,
    alerted: usize,
    calmed: usize,
    /// Frames of paced creatures playing a movement clip, and changes between a walk
    /// and a run clip among them (the stop-and-start of a speed on the switch point).
    moving_frames: usize,
    swaps: usize,
    last: std::collections::HashMap<u32, bool>,
}

/// A clip set's walk and run speeds for a creature: its clips' root motion × `scale` ×
/// `speed_mult` (percent) × its skeleton's `share`. Without a run clip it runs at its walk
/// clip's pace [observed: the AI observer in the original game, 2026-10-25,
/// `creature_flee`: told to run (an Always Run travel package), Dead Money's ghost people
/// ran a mean of 105.4 against their walk's 104.5, its holograms 102.9 against 102.3, Old
/// World Blues' 86.1 against 85.3]; a move without a clip keeps `default`'s speed. The
/// lakelurk has a run clip, `locomotion\mtfastforward.kf` (sequence `Fastforward`,
/// 355.95), and ran a mean of 358 over 3.9 s [observed, the same campaign].
pub fn clip_speeds(set: &ClipSet, scale: f32, speed_mult: f32, share: Share, default: Speeds) -> Speeds {
    let k = scale * speed_mult / 100.0;
    let clip = |m: Move| set.info(m).map(|i| i.speed).filter(|v| *v > 1.0);
    let walk = clip(Move::Forward).map(|v| v * k * share.walk);
    let run = clip(Move::FastForward).map(|v| v * k * share.run).or(walk);
    Speeds { walk: walk.unwrap_or(default.walk), run: run.unwrap_or(default.run) }
}

/// The share of the people's speeds (`fnv_ai::NPC_WALK`, `NPC_RUN`) a person keeps for
/// its body armour (`fnv_game::combat::armor_speed`: medium × 0.925, heavy × 0.85
/// [observed, actors.md §8.10]), on top of its Speed Mult's ([`paced_speeds`]). [observed
/// against the Speed Mult part, one actor: Cook-Cook, Speed Mult 110, ran 255 in metal
/// armour with the Incinerator drawn, 344 × 0.75 = 258; with his Speed Mult 284; asked in
/// docs/owner-review.md.]
pub fn person_pace(game: &fnv_game::Game, world: &fnv_sim::World, id: FormId) -> f32 {
    fnv_game::combat::armor_speed(game, fnv_game::combat::worn_armor_class(world, id)) as f32
}

/// How often a drawn person's armour is looked at again, seconds: scripts and the
/// person's own outfitting change what it wears [design].
const PERSON_PACE_EVERY: f32 = 1.0;

/// How often a drawn actor's current Speed Mult is read again, seconds [design: as the
/// legs' conditions, `limping.rs`; a script's SetAV or an effect's start is taken within
/// a quarter second, without reading every actor's values every frame].
const SPEED_MULT_EVERY: f32 = 0.25;

/// The run speeds given, for GetRunSpeed (`fnv_game::nvse_jg9`).
fn tell_run_speeds(script: Option<&mut ScriptState>, given: &[(FormId, ActorSpeeds)]) {
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return };
    for (id, s) in given {
        rt.game.other.jg9.run_speeds.insert(*id, s.calm.run);
    }
}

/// Run `f` on the game the drawn actors live in: the script runtime's, with the played
/// world swapped in (`nvf_play::script::with_world`: a reference a script made, PlaceAtMe,
/// is only in that one), else the residents' own game; with the world their armour is read
/// from when it is not the game's own (`None`: the game's).
fn on_game<R>(script: Option<&mut ScriptState>, sim: Option<&mut SimState>, res: &mut Residents, f: impl FnOnce(&mut fnv_game::Game<'static>, Option<&fnv_sim::World<'static>>) -> R) -> Option<R> {
    match (script.and_then(|s| s.rt.as_mut()), sim) {
        (Some(rt), Some(sim)) => Some(with_world(rt, sim, |rt| f(&mut rt.game, None))),
        (Some(rt), None) => Some(f(&mut rt.game, None)),
        (None, sim) => {
            let world = sim.map(|s| &s.world);
            res.own_game().map(|g| f(g, world))
        }
    }
}

/// The speeds `paced` gives, worked out afresh from the unscaled ones every time (the
/// people's `default` speeds, the creature's clips), so a change never multiplies on an
/// earlier one; and the clips' pace (`Locomotion::pace`): Speed Mult (× the skeleton's
/// walking share for a creature). People: `default` × Speed Mult × the armour's share;
/// creatures: [`clip_speeds`] of the calm and alert sets, turning at their own rate. A
/// Speed Mult of 0 gives speeds of 0: the actor stands where it is (the AI keeps its walk,
/// `fnv_ai::Ai::set_speeds`). `None` for [`PaceKind::Own`].
fn paced_speeds(paced: &Paced, lo: &mut Locomotion, default: Speeds) -> Option<ActorSpeeds> {
    let f = paced.mult / 100.0;
    match paced.kind {
        PaceKind::Person { armour } => {
            // Its clips keep up with its Speed Mult, not its armour (module notes).
            lo.pace = f;
            let s = Speeds { walk: default.walk * f * armour, run: default.run * f * armour };
            Some(ActorSpeeds { calm: s, alert: s, turn_rate: None, spot_turn_start: None })
        }
        PaceKind::Creature { scale, share, turn, .. } => {
            // Its skeleton's share; the clips play at that pace.
            lo.pace = f * share.walk;
            let calm = clip_speeds(&lo.stand, scale, paced.mult, share, default);
            let alert = lo.alert.as_deref().map_or(calm, |a| clip_speeds(a, scale, paced.mult, share, default));
            Some(ActorSpeeds { calm, alert, turn_rate: Some(turn.to_radians()), spot_turn_start: Some(CREATURE_SPOT_TURN_START) })
        }
        PaceKind::Own => None,
    }
}

/// Look at `paced` again with its current Speed Mult `mult` and, when read, a person's
/// armour share `armour`: the new speeds when either changed ([`paced_speeds`]), `None`
/// otherwise (issue #2118: the speeds were given once, and only an armour change gave
/// them again).
fn repace(paced: &mut Paced, mult: f32, armour: Option<f32>, lo: &mut Locomotion, default: Speeds) -> Option<ActorSpeeds> {
    let mut changed = (mult - paced.mult).abs() >= 1e-3;
    paced.mult = mult;
    if let (PaceKind::Person { armour: was }, Some(now)) = (&mut paced.kind, armour) {
        if (now - *was).abs() >= 1e-3 {
            *was = now;
            changed = true;
        }
    }
    if changed {
        paced_speeds(paced, lo, default)
    } else {
        None
    }
}

/// The log line for a creature's speeds `s`.
fn creature_line(name: &str, id: FormId, paced: &Paced, s: &ActorSpeeds, has_alert: bool) -> String {
    let PaceKind::Creature { scale, turn, tnam, .. } = paced.kind else { return String::new() };
    format!(
        "creature pace: {name} {id} walks {:.0} and runs {:.0} units/s, turns {turn:.0} degrees/s{} (scale {scale:.2}, Speed Mult {:.0}, TNAM {})",
        s.calm.walk,
        s.calm.run,
        if has_alert { format!(", alert {:.0} and {:.0}", s.alert.walk, s.alert.run) } else { String::new() },
        paced.mult,
        tnam.map_or("none".to_string(), |t| format!("{t:.0}"))
    )
}

/// A newly drawn actor's pace and the speeds it is given: always, at Speed Mult 100 and
/// armour 1 too, as the AI may still hold the speeds it had when it was last drawn (a
/// despawn takes [`Paced`] off but leaves `fnv_ai::Ai::speeds`), at another Speed Mult.
/// `None` for the player or an actor without a record ([`PaceKind::Own`]). A person's
/// armour share is read from `armour` (`Some(None)`: the game's own world) when given,
/// else 1 until [`refresh_speeds`] reads it. Logs the `npc pace` (Speed Mult not 100),
/// `person pace` (armour not 1) and `creature pace` lines.
#[allow(clippy::too_many_arguments)]
fn first_pace(
    game: &mut fnv_game::Game<'_>,
    base: FormId,
    id: FormId,
    skeleton: &str,
    scale: f32,
    armour: Option<Option<&fnv_sim::World<'_>>>,
    lo: &mut Locomotion,
    default: Speeds,
    stats: &mut PaceStats,
) -> (Paced, Option<ActorSpeeds>) {
    let creature = game.data.actors.get(&base).map(|def| &def.sig == b"CREA");
    let Some(creature) = creature.filter(|_| id != FormId::PLAYER_REF) else {
        return (Paced { mult: 100.0, kind: PaceKind::Own }, None);
    };
    let mult = current_speed_mult(game, base, id);
    if !creature {
        let share = armour.map_or(1.0, |w| person_pace(game, w.unwrap_or(&game.world), id));
        let paced = Paced { mult, kind: PaceKind::Person { armour: share } };
        let s = paced_speeds(&paced, lo, default);
        if let Some(s) = s {
            if (mult - 100.0).abs() >= 0.5 {
                info!("npc pace: {} {id} walks {:.0} and runs {:.0} units/s (Speed Mult {mult:.0})", game.data.name(id), s.calm.walk, s.calm.run);
                stats.npcs += 1;
                stats.paced += 1;
            }
            if (share - 1.0).abs() >= 1e-3 {
                info!("person pace: {} {id} walks {:.0} and runs {:.0} units/s ({:.1}% of the people's speeds for its armour)", game.data.name(base), s.calm.walk, s.calm.run, share * 100.0);
                stats.people += 1;
            }
        }
        return (paced, s);
    }
    let share = skeleton_share(skeleton);
    let tnam = game.creature_turning_speed(base, id);
    let paced = Paced { mult, kind: PaceKind::Creature { scale, share, turn: creature_turn_rate(tnam, scale), tnam } };
    let s = paced_speeds(&paced, lo, default);
    if let Some(s) = &s {
        info!("{}", creature_line(&game.data.name(base), id, &paced, s, lo.alert.is_some()));
        stats.paced += 1;
    }
    (paced, s)
}

/// Drawn actors are given their speeds once their clips are loaded ([`first_pace`]).
fn give_speeds(
    mut commands: Commands,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: Option<ResMut<SimState>>,
    mut res: Option<ResMut<Residents>>,
    mut actors: Query<(Entity, &ActorRig, &mut Locomotion, &GlobalTransform), Without<Paced>>,
    mut stats: ResMut<PaceStats>,
) {
    let Some(res) = res.as_deref_mut() else { return };
    if actors.is_empty() {
        return;
    }
    let default = res.default_speeds();
    // Without the simulation there is no worn armour to read (as in `refresh_speeds`).
    let have_sim = sim.is_some();
    let given = on_game(script.as_deref_mut(), sim.as_deref_mut(), res, |game, world| {
        let mut given = Vec::new();
        for (e, rig, mut lo, global) in &mut actors {
            let (id, base) = (FormId(rig.reference), FormId(rig.base));
            let scale = global.affine().transform_vector3(Vec3::Z).length().max(0.01);
            let (paced, s) = first_pace(game, base, id, &rig.skeleton, scale, have_sim.then_some(world), &mut lo, default, &mut stats);
            if matches!(paced.kind, PaceKind::Creature { .. }) {
                commands.entity(e).insert((paced, PacedCreature));
            } else {
                commands.entity(e).insert(paced);
            }
            given.extend(s.map(|s| (id, s)));
        }
        given
    });
    let Some(given) = given else { return };
    tell_run_speeds(script.as_deref_mut(), &given);
    for (id, s) in given {
        res.set_speeds(id, s);
    }
}

/// Paced actors are given their speeds again when their current Speed Mult changes
/// (SetAV, ModAV, ForceAV, an effect, damage or its restoration; read every
/// [`SPEED_MULT_EVERY`]), people also when their armour's share does (read every
/// [`PERSON_PACE_EVERY`]). The AI takes the new speeds at once, a walk under way too
/// (`fnv_ai::Ai::set_speeds`).
#[allow(clippy::too_many_arguments)]
fn refresh_speeds(
    time: Res<Time>,
    mut waits: Local<(f32, f32)>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: Option<ResMut<SimState>>,
    mut res: Option<ResMut<Residents>>,
    mut actors: Query<(&ActorRig, &mut Locomotion, &mut Paced)>,
    mut stats: ResMut<PaceStats>,
) {
    let dt = time.delta_secs();
    let (mult_wait, armour_wait) = &mut *waits;
    *mult_wait -= dt;
    *armour_wait -= dt;
    let armour_due = *armour_wait <= 0.0;
    if *mult_wait > 0.0 && !armour_due {
        return;
    }
    *mult_wait = SPEED_MULT_EVERY;
    if armour_due {
        *armour_wait = PERSON_PACE_EVERY;
    }
    let Some(res) = res.as_deref_mut() else { return };
    let default = res.default_speeds();
    // Without the simulation there is no worn armour to read (as before).
    let armour_due = armour_due && sim.is_some();
    let given = on_game(script.as_deref_mut(), sim.as_deref_mut(), res, |game, world| {
        let mut given = Vec::new();
        for (rig, mut lo, mut paced) in &mut actors {
            if paced.kind == PaceKind::Own {
                continue;
            }
            let (id, base) = (FormId(rig.reference), FormId(rig.base));
            let mult = current_speed_mult(game, base, id);
            let person = matches!(paced.kind, PaceKind::Person { .. });
            let armour = (person && armour_due).then(|| person_pace(game, world.unwrap_or(&game.world), id));
            let before = *paced;
            let Some(s) = repace(&mut paced, mult, armour, &mut lo, default) else { continue };
            if (mult - before.mult).abs() >= 1e-3 {
                stats.repaced += 1;
                match paced.kind {
                    PaceKind::Person { armour } => info!(
                        "npc pace: {} {id} walks {:.0} and runs {:.0} units/s (Speed Mult {mult:.0} now, {:.0} before; {:.1}% of the people's speeds for its armour)",
                        game.data.name(id),
                        s.calm.walk,
                        s.calm.run,
                        before.mult,
                        armour * 100.0
                    ),
                    _ => info!("{}, Speed Mult {:.0} before", creature_line(&game.data.name(base), id, &paced, &s, lo.alert.is_some()), before.mult),
                }
            }
            if let (PaceKind::Person { armour }, PaceKind::Person { armour: was }) = (paced.kind, before.kind) {
                if (armour - was).abs() >= 1e-3 {
                    info!("person pace: {} {id} walks {:.0} and runs {:.0} units/s ({:.1}% of the people's speeds for its armour)", game.data.name(base), s.calm.walk, s.calm.run, armour * 100.0);
                    stats.people += 1;
                }
            }
            given.push((id, s));
        }
        given
    });
    let Some(given) = given else { return };
    tell_run_speeds(script.as_deref_mut(), &given);
    for (id, s) in given {
        res.set_speeds(id, s);
    }
}

/// An actor's current Speed Mult, percent: its base (the record's ACBS field through the
/// template chain (Stats), as GetBaseAV gives it, or SetAV's number), its permanent and
/// temporary modifiers (ModAV, ForceAV, effects) less its damage (DamageAV, RestoreAV), as
/// GetAV reads it (`fnv_game::functions::actor_value`) and as the player's speed takes it
/// (`fnv_game::Game::move_speed_mult`; issues #1364, #1742, and #2118 for people and
/// creatures). A record whose own field is 0 or less counts as 100, as before [design:
/// taken for a record without a value]; a 0 from SetAV, the modifiers or damage stands,
/// and the actor stands still. Between 0 and 1000, as the player's (× 10). Without the
/// reference in the game's world (outside `with_world`) its base's stats source is read.
pub fn current_speed_mult(game: &mut fnv_game::Game<'_>, base: FormId, id: FormId) -> f32 {
    let code = fnv_game::actor_values::SPEED_MULTIPLIER;
    let v = if fnv_game::functions::base_of(game, id).is_some() {
        let current = fnv_game::functions::actor_value(game, id, code).ok();
        let set = game.extra.get(&id).is_some_and(|e| e.actor_values.contains_key(&code));
        let empty_record = if set { None } else { game.base_actor_value(id, code).ok().filter(|r| *r <= 0.0) };
        current.map(|c| c + empty_record.map_or(0.0, |r| 100.0 - r))
    } else {
        game.template_source(base, id, |d| d.template_stats).map(|(_, d)| d.speed_mult as f64).map(|v| if v > 0.0 { v } else { 100.0 })
    };
    v.filter(|v| v.is_finite()).map_or(100.0, |v| v.clamp(0.0, 1000.0) as f32)
}

/// Creatures with an alert stance take it outside the Normal detection state.
fn stance(script: Option<Res<ScriptState>>, mut actors: Query<(&ActorRig, &mut Locomotion), With<PacedCreature>>, mut stats: ResMut<PaceStats>) {
    let Some(game) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    for (rig, mut lo) in &mut actors {
        let id = FormId(rig.reference);
        let mv = lo.blend.cur.mv;
        if mv.is_moving() {
            stats.moving_frames += 1;
            let fast = matches!(mv, Move::FastForward | Move::FastBackward | Move::FastLeft | Move::FastRight);
            if stats.last.insert(rig.reference, fast).is_some_and(|was| was != fast) {
                stats.swaps += 1;
                if stats.swaps <= SWAPS_LOGGED {
                    let v = lo.velocity[0].hypot(lo.velocity[1]);
                    info!(
                        "creature pace: {} {id} changes to its {} clip at {v:.0} units/s in its own scale ({:.0} at its clips' pace){}",
                        game.data.name(FormId(rig.base)),
                        if fast { "run" } else { "walk" },
                        v / lo.pace.max(0.01),
                        if game.awareness.state(id) != State::Normal { ", alert" } else { "" }
                    );
                }
            }
        }
        if lo.alert.is_none() {
            continue;
        }
        let want = game.awareness.state(id) != State::Normal;
        if lo.set_alert(want) {
            if want {
                stats.alerted += 1;
            } else {
                stats.calmed += 1;
            }
            info!("creature pace: {} {id} {}", game.data.name(FormId(rig.base)), if want { "takes its alert stance" } else { "is calm again" });
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, stats: Res<PaceStats>) {
    if exit.read().next().is_some() {
        info!(
            "creature pace summary: {} creatures given their own speeds ({} people by Speed Mult), people given their armour's speeds {} times; alert stance taken {} times and left {} times; {} frames of their moves, {} changes between walk and run clips; speeds given again for a new Speed Mult {} times",
            stats.paced - stats.npcs, stats.npcs, stats.people, stats.alerted, stats.calmed, stats.moving_frames, stats.swaps, stats.repaced
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_actor::locomotion::ClipInfo;

    #[test]
    fn creatures_turn_at_their_tnam() {
        // Measured in the original (campaign o4turn): steady big turns at TNAM × 1.25,
        // the base scale playing no part.
        let near = |got: f32, seen: f32| assert!((got - seen).abs() <= seen * 0.01, "{got} against {seen}");
        near(creature_turn_rate(Some(100.0), 1.0), 125.0); // CrMoleRat
        near(creature_turn_rate(Some(150.0), 1.0), 187.5); // CrCoyote
        near(creature_turn_rate(Some(75.0), 1.0), 93.75); // CrDeathClaw
        near(creature_turn_rate(Some(720.0), 1.0), 900.2); // NVCrCazador
        near(creature_turn_rate(Some(200.0), 0.8), 250.0); // VCrCazadorTier1TypeA
        near(creature_turn_rate(Some(90.0), 1.05), 112.5); // VCrNightStalkerTier1TypeB
        // TNAM 0 (or none): the fitted rate × scale.
        assert_eq!(creature_turn_rate(Some(0.0), 0.5), CREATURE_TURN_RATE * 0.5);
        assert_eq!(creature_turn_rate(None, 1.0), CREATURE_TURN_RATE);
        assert_eq!(creature_turn_rate(Some(f32::NAN), 1.0), CREATURE_TURN_RATE);
    }

    #[test]
    fn skeletons_keep_their_measured_shares() {
        let s = |p: &str| skeleton_share(p);
        let whole = Share { walk: 1.0, run: 1.0 };
        assert_eq!(s("creatures\\nvcazadores\\skeleton.nif"), Share { walk: 0.70, run: 0.70 });
        assert_eq!(s("meshes/Creatures/NVCazadores/Skeleton.nif").run, 0.70);
        assert_eq!(s("Creatures\\Protectron\\skeleton.nif"), Share { walk: 0.88, ..whole });
        for other in ["Creatures\\MirelurkKing\\Skeleton.nif","Creatures\\Brahmin\\Skeleton.nif", "Creatures\\MoleRat\\Skeleton.nif", "creatures\\NVgiantrat\\skeleton.nif",
                      "Creatures\\Dog\\Skeleton.nif", "Creatures\\DeathClaw\\Skeleton.nif", "Creatures\\Ghoul\\Skeleton.nif",
                      "Creatures\\MisterGutsy\\Skeleton.nif", "Creatures\\SentryBot\\Skeleton.nif",
                      "NVDLC03\\Creatures\\Roboscorpion\\skeleton_RoboScopion.nif", "NVDLC01\\Creatures\\Ghosts\\skeleton.nif"] {
            assert_eq!(s(other), whole, "{other}");
        }
        // Measured in the original (campaign creature_runs): the dog (scale 0.8) charges on
        // its h2hfastforward at 363 and walks 45.9; the coyote (Speed Mult 110) charges at
        // 500; the radscorpion (0.6, 150) runs 281 and walks 67.0; the feral ghoul runs 388
        // and walks 97; the deathclaw (1.0) runs about 600 and walks 120.
        let d = Speeds { walk: 85.3, run: 344.0 };
        let near = |got: f32, seen: f32| assert!((got - seen).abs() <= seen * 0.03, "{got} against {seen}");
        let dog = clip_speeds(&set(57.36, Some(454.75)), 0.8, 100.0, s("creatures\\dog\\skeleton.nif"), d);
        near(dog.run, 363.0);
        near(dog.walk, 45.9);
        near(clip_speeds(&set(57.36, Some(454.75)), 1.0, 110.0, s("creatures\\dog\\skeleton.nif"), d).run, 500.0);
        let rad = clip_speeds(&set(74.9, Some(315.04)), 0.6, 150.0, s("creatures\\radscorpion\\skeleton.nif"), d);
        near(rad.run, 281.0);
        near(rad.walk, 67.0);
        let ghoul = clip_speeds(&set(95.72, Some(384.0)), 1.0, 100.0, s("creatures\\ghoul\\skeleton.nif"), d);
        near(ghoul.run, 388.0);
        near(ghoul.walk, 97.0);
        let claw = clip_speeds(&set(120.0, Some(599.99)), 1.0, 100.0, s("creatures\\deathclaw\\skeleton.nif"), d);
        near(claw.run, 600.0);
        near(claw.walk, 120.0);
        // Measured (campaign creature_flee): the protectron walks 22.2 and runs 84.4; ghost
        // people (scale 1.25), with no run clip, run at their walk's pace, 105.4; the
        // lakelurk runs 358 on its mtfastforward (355.95).
        let pro = clip_speeds(&set(25.25, Some(83.37)), 1.0, 100.0, s("creatures\\protectron\\skeleton.nif"), d);
        near(pro.walk, 22.2);
        near(pro.run, 84.4);
        let ghost = clip_speeds(&set(83.62, None), 1.25, 100.0, s("nvdlc01\\creatures\\ghosts\\skeleton.nif"), d);
        near(ghost.run, 105.4);
        near(clip_speeds(&set(30.16, Some(355.95)), 1.0, 100.0, s("creatures\\mirelurkking\\skeleton.nif"), d).run, 358.0);
        // Measured (campaign creature_runs_all): the alien walks 86–91 and runs 420.8 on its
        // forwardwalk (89.89) and fastforward (419.9); the nightstalker walks 100.6 and
        // runs 498.8 on its mtfoward (100.7) and mtfastfoward (500.0); the sentry bot
        // walks 96.2 and runs 383.7 on 96.0 and 384.0.
        let alien = clip_speeds(&set(89.89, Some(419.9)), 1.0, 100.0, s("dlc05\\creatures\\alien\\skeleton.nif"), d);
        near(alien.walk, 88.5);
        near(alien.run, 420.8);
        let stalker = clip_speeds(&set(100.7, Some(500.0)), 1.0, 100.0, s("creatures\\nightstalker\\skeleton.nif"), d);
        near(stalker.walk, 100.6);
        near(stalker.run, 498.8);
        let sentry = clip_speeds(&set(96.0, Some(384.0)), 1.0, 100.0, s("creatures\\sentrybot\\skeleton.nif"), d);
        near(sentry.walk, 96.2);
        near(sentry.run, 383.7);
    }

    #[test]
    fn the_sonic_bark_cyberdog_keeps_its_calm_clips_when_alert() {
        // Observed: NVDLC03CrCyberDogMilitary01Ranged charged at 333.1 on the mt run; the
        // plain dog charges on the h2h run.
        assert!(!has_alert_stance("meshes\\creatures\\dog\\skeleton_sonicbark.nif"));
        assert!(!has_alert_stance("Creatures\\Dog\\Skeleton_SonicBark.NIF"));
        for other in ["meshes\\creatures\\dog\\skeleton.nif", "creatures\\nvcazadores\\skeleton.nif", "creatures\\mirelurkking\\skeleton.nif"] {
            assert!(has_alert_stance(other), "{other}");
        }
        let d = Speeds { walk: 85.3, run: 344.0 };
        let share = skeleton_share("creatures\\dog\\skeleton_sonicbark.nif");
        assert!((clip_speeds(&set(57.36, Some(333.07)), 1.0, 100.0, share, d).run - 333.1).abs() < 333.1 * 0.03);
    }
    use nvf_assets::anim_play::{BoundClip, Clip};
    use std::sync::Arc;

    fn set(walk: f32, run: Option<f32>) -> ClipSet {
        let bound = Arc::new(BoundClip {
            clip: Arc::new(Clip {
                name: String::new(),
                start: 0.0,
                stop: 1.0,
                frequency: 1.0,
                cycle_type: 0,
                tracks: Vec::new(),
                priorities: Vec::new(),
                skipped: 0,
                accum_root: None,
                text_keys: Vec::new(),
                visibility: Vec::new(),
                emitters: Vec::new(),
                materials: Vec::new(),
                free: Vec::new(),
                free_visibility: Vec::new(),
                lights: Vec::new(),
            }),
            tracks: Vec::new(),
            root: None,
            root_track: None,
            carried: None,
        });
        let clip = |speed: f32| crate::locomotion::SetClip { bound: bound.clone(), info: ClipInfo { duration: 1.0, speed, blend_time: 0.1, foot_phase: 0.0 }, path: String::new() };
        let mut s = ClipSet::default();
        s.clips.insert(Move::Forward, clip(walk));
        if let Some(r) = run {
            s.clips.insert(Move::FastForward, clip(r));
        }
        s
    }

    #[test]
    fn cazadors_move_at_the_observed_speeds() {
        let d = Speeds { walk: 86.8, run: 344.0 };
        // The cazador's mtforward.kf and mtfastforward.kf (216.73 and 545.54 units/s).
        let caz = set(216.73, Some(545.54));
        let share = skeleton_share("creatures\\nvcazadores\\skeleton.nif");
        let young = clip_speeds(&caz, 0.5, 300.0, share, d);
        let medium = clip_speeds(&caz, 1.0, 250.0, share, d);
        // Observed: walks 228 and 380, charges up to 576 and 974.
        assert!((young.walk - 228.0).abs() < 5.0, "{young:?}");
        assert!((medium.walk - 380.0).abs() < 5.0, "{medium:?}");
        assert!((young.run - 576.0).abs() < 10.0, "{young:?}");
        assert!((medium.run - 974.0).abs() < 30.0, "{medium:?}");
        // The medium cazador's walk plays its walk clip, at 1.75 its rate; without the
        // pace it would count as running.
        let mut lo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        lo.velocity = [0.0, medium.walk];
        lo.step(0.016);
        assert_eq!(lo.blend.cur.mv, Move::FastForward);
        let mut lo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        lo.pace = 2.5 * share.walk;
        lo.velocity = [0.0, medium.walk];
        lo.step(0.016);
        assert_eq!(lo.blend.cur.mv, Move::Forward);
        assert!((lo.blend.cur.rate - 1.75).abs() < 0.01, "{}", lo.blend.cur.rate);
        // Near the switch point (343.9) the gait holds: walk until 395, run down to 299.
        let mut lo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        let mut seen = Vec::new();
        for v in [216.0, 360.0, 420.0, 330.0, 250.0] {
            lo.velocity = [0.0, v];
            // Past any cross-fade, so the chosen move is taken.
            for _ in 0..60 {
                lo.step(0.016);
            }
            seen.push(lo.blend.cur.mv);
        }
        assert_eq!(seen, [Move::Forward, Move::Forward, Move::FastForward, Move::FastForward, Move::Forward]);
        // A one-frame spike to three times the walk does not change the gait.
        let mut lo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        let mut gaits = Vec::new();
        for v in [216.0; 30].into_iter().chain([650.0]).chain([216.0; 30]) {
            lo.velocity = [0.0, v];
            lo.step(1.0 / 60.0);
            gaits.push(lo.blend.cur.mv);
        }
        assert!(gaits.iter().all(|m| *m == Move::Forward), "{gaits:?}");
        // No run clip: the walk's pace.
        assert!((clip_speeds(&set(100.0, None), 1.0, 100.0, share, d).run - 70.0).abs() < 0.01);
    }

    const NPC: FormId = FormId(1);
    const CREA: FormId = FormId(2);
    /// A person whose record has no Speed Mult (0).
    const BLANK: FormId = FormId(3);
    const D: Speeds = Speeds { walk: 85.3, run: 344.0 };

    /// A game with a person (record Speed Mult 100), a creature (150) and a person whose
    /// record has none (0), each placed in the game's world as PlaceAtMe places one.
    fn speed_game() -> (fnv_game::Game<'static>, [FormId; 3]) {
        use fnv_sim::forms::{ActorBase, BaseData, BaseForm};
        let mut forms = fnv_sim::FormStore::default();
        for (id, sig) in [(NPC, *b"NPC_"), (CREA, *b"CREA"), (BLANK, *b"NPC_")] {
            forms.sigs.insert(id, sig);
            forms.bases.insert(id, BaseForm { id, sig, edid: None, name: None, script: None, prompt: None, voice_filter: false, data: BaseData::Actor(ActorBase { entries: vec![], attributes: None }) });
        }
        let forms: &'static fnv_sim::FormStore = Box::leak(Box::new(forms));
        let refs: &'static fnv_sim::RefTable = Box::leak(Box::default());
        let empty = crate::outfits::tests::empty_world_data();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
        let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let table = fnv_script::FunctionTable::load(&path).unwrap();
        let mut data = fnv_game::GameData::build(empty.lo, empty.schema, table, &json, refs);
        for (id, sig, speed_mult) in [(NPC, *b"NPC_", 100), (CREA, *b"CREA", 150), (BLANK, *b"NPC_", 0)] {
            data.actors.insert(id, fnv_game::data::ActorDef { sig, speed_mult, ..Default::default() });
        }
        let data: &'static fnv_game::GameData = Box::leak(Box::new(data));
        let mut game = fnv_game::Game::new(data, fnv_sim::World::new(forms, refs, Default::default()), Default::default());
        let ids = [NPC, CREA, BLANK].map(|b| game.world.place(b, None, [0.0; 3], [0.0; 3], 1).unwrap());
        (game, ids)
    }

    /// Call the script function `name` on `on` with an actor value and a number.
    fn call(game: &mut fnv_game::Game<'static>, name: &str, on: FormId, code: u16, n: f64) {
        use fnv_script::vm::{ArgValue, Value};
        let opcode = game.data.table.by_name(name).unwrap();
        let ctx = fnv_game::functions::Ctx { unit: "test".into(), opcode, on: Some(on), self_ref: Some(on), action_ref: None, quest: None, inst: None, in_condition: false };
        let from = game.events.len();
        game.call_function(&ctx, &[ArgValue::Enum(code), ArgValue::Value(Value::Num(n))]);
        assert!(!game.events[from..].iter().any(|e| matches!(e.kind, fnv_game::trace::EventKind::CallFailed { .. })), "{name} failed");
    }

    /// The speeds given when `id`'s current Speed Mult is looked at again, as
    /// `refresh_speeds` looks (its armour not read).
    fn look_again(game: &mut fnv_game::Game<'static>, base: FormId, id: FormId, paced: &mut Paced, lo: &mut Locomotion) -> Option<ActorSpeeds> {
        let mult = current_speed_mult(game, base, id);
        repace(paced, mult, None, lo, D)
    }

    fn near(got: f32, want: f32) {
        assert!((got - want).abs() < 0.05, "{got} against {want}");
    }

    /// Issue #2118: a person and a creature already paced take their current Speed Mult
    /// (SetAV, ModAV, an effect, DamageAV, RestoreAV) without an armour change, worked out
    /// afresh each time; 0 stands still and is not read as 100.
    #[test]
    fn paced_actors_follow_their_current_speed_mult() {
        let (mut game, [person, creature, _]) = speed_game();
        let sm = fnv_game::actor_values::SPEED_MULTIPLIER;
        // A person in medium armour (92.5 % of the people's speeds), paced at 100.
        let mut plo = Locomotion::new(Arc::new(set(85.3, Some(344.0))), 0.0);
        let mut p = Paced { mult: current_speed_mult(&mut game, NPC, person), kind: PaceKind::Person { armour: 0.925 } };
        assert_eq!(p.mult, 100.0);
        let first = paced_speeds(&p, &mut plo, D).unwrap();
        near(first.calm.run, 344.0 * 0.925);
        assert_eq!(look_again(&mut game, NPC, person, &mut p, &mut plo), None, "nothing changed");
        let run = |s: Option<ActorSpeeds>| s.expect("given again").calm.run;
        // SetAV 200.
        call(&mut game, "SetActorValue", person, sm, 200.0);
        let at_200 = look_again(&mut game, NPC, person, &mut p, &mut plo).expect("given again");
        near(at_200.calm.walk, 85.3 * 2.0 * 0.925);
        near(at_200.calm.run, 344.0 * 2.0 * 0.925);
        assert_eq!(at_200.alert, at_200.calm);
        assert_eq!((at_200.turn_rate, plo.pace), (None, 2.0));
        assert_eq!(look_again(&mut game, NPC, person, &mut p, &mut plo), None, "not multiplied again");
        // ModAV -50: 150.
        call(&mut game, "ModActorValue", person, sm, -50.0);
        near(run(look_again(&mut game, NPC, person, &mut p, &mut plo)), 344.0 * 1.5 * 0.925);
        // An effect's +30: 180.
        game.extra.entry(person).or_default().av_temporary.insert(sm, 30.0);
        let at_180 = look_again(&mut game, NPC, person, &mut p, &mut plo).expect("given again");
        near(at_180.calm.run, 344.0 * 1.8 * 0.925);
        // DamageAV 180: 0, standing still, not 100.
        call(&mut game, "DamageActorValue", person, sm, 180.0);
        let still = look_again(&mut game, NPC, person, &mut p, &mut plo).expect("given again");
        assert_eq!((still.calm.walk, still.calm.run, p.mult), (0.0, 0.0, 0.0));
        // RestoreAV 90, then the rest: back to 180, the same speeds as before the damage.
        call(&mut game, "RestoreActorValue", person, sm, 90.0);
        near(run(look_again(&mut game, NPC, person, &mut p, &mut plo)), 344.0 * 0.9 * 0.925);
        call(&mut game, "RestoreActorValue", person, sm, 1000.0);
        assert_eq!(look_again(&mut game, NPC, person, &mut p, &mut plo), Some(at_180));
        assert!((plo.pace - 1.8).abs() < 1e-5, "{}", plo.pace);
        // Its armour changing alone still gives its speeds again, on the current Speed Mult.
        let mult = current_speed_mult(&mut game, NPC, person);
        near(repace(&mut p, mult, Some(0.85), &mut plo, D).expect("armour").calm.run, 344.0 * 1.8 * 0.85);

        // A cazador (scale 1, Speed Mult 150), paced from its clips.
        let caz = set(216.73, Some(545.54));
        let mut clo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        let share = skeleton_share("creatures\\nvcazadores\\skeleton.nif");
        let mut c = Paced { mult: current_speed_mult(&mut game, CREA, creature), kind: PaceKind::Creature { scale: 1.0, share, turn: 250.0, tnam: Some(200.0) } };
        assert_eq!(c.mult, 150.0);
        paced_speeds(&c, &mut clo, D).unwrap();
        assert_eq!(look_again(&mut game, CREA, creature, &mut c, &mut clo), None);
        call(&mut game, "SetActorValue", creature, sm, 300.0);
        let at_300 = look_again(&mut game, CREA, creature, &mut c, &mut clo).expect("given again");
        assert_eq!(at_300.calm, clip_speeds(&caz, 1.0, 300.0, share, D));
        assert_eq!(at_300.turn_rate, Some(250f32.to_radians()));
        assert!((clo.pace - 3.0 * 0.70).abs() < 1e-5, "{}", clo.pace);
        call(&mut game, "DamageActorValue", creature, sm, 300.0);
        let still = look_again(&mut game, CREA, creature, &mut c, &mut clo).expect("given again");
        assert_eq!((still.calm.walk, still.calm.run), (0.0, 0.0));
        call(&mut game, "RestoreActorValue", creature, sm, 300.0);
        assert_eq!(look_again(&mut game, CREA, creature, &mut c, &mut clo), Some(at_300));
    }

    /// Review of #2118: an actor that left view at Speed Mult 200 (or 0) and is drawn again
    /// after its value came back to 100 is given 100's speeds: the despawn took its
    /// `Paced` off but the AI kept the old speeds, and nothing was given at 100.
    #[test]
    fn an_actor_drawn_again_at_100_loses_the_speeds_it_left_with() {
        let (mut game, [person, creature, _]) = speed_game();
        let sm = fnv_game::actor_values::SPEED_MULTIPLIER;
        let mut ai = fnv_ai::Ai::new(fnv_ai::AiConfig { walk_speed: D.walk, run_speed: D.run, ..Default::default() });
        let mut stats = PaceStats::default();
        let mut lo = Locomotion::new(Arc::new(set(85.3, Some(344.0))), 0.0);
        // Drawn at 100 in no armour: given the people's speeds, which the AI has anyway.
        let (mut p, s) = first_pace(&mut game, NPC, person, "", 1.0, None, &mut lo, D, &mut stats);
        assert_eq!(s.map(|s| s.calm), Some(D));
        ai.set_speeds(person, s.unwrap());
        // SetAV 200 while drawn, then it leaves view: the AI keeps the doubled speeds.
        call(&mut game, "SetActorValue", person, sm, 200.0);
        ai.set_speeds(person, look_again(&mut game, NPC, person, &mut p, &mut lo).expect("given again"));
        near(ai.calm_speeds(person).run, 688.0);
        // (The despawn takes its `Paced` off.)
        // Back to 100 while away; drawn again: 100's speeds, given though nothing differs
        // from a first pace at 100.
        call(&mut game, "SetActorValue", person, sm, 100.0);
        let mut lo = Locomotion::new(Arc::new(set(85.3, Some(344.0))), 0.0);
        let (p, s) = first_pace(&mut game, NPC, person, "", 1.0, None, &mut lo, D, &mut stats);
        assert_eq!(p.mult, 100.0);
        ai.set_speeds(person, s.expect("given at 100 too"));
        assert_eq!(ai.calm_speeds(person), D);
        assert_eq!(lo.pace, 1.0);
        // A creature that left at 0 is drawn again at its record's 150: its clips' speeds.
        let caz = set(216.73, Some(545.54));
        let skeleton = "creatures\\nvcazadores\\skeleton.nif";
        let mut clo = Locomotion::new(Arc::new(caz.clone()), 0.0);
        call(&mut game, "SetActorValue", creature, sm, 0.0);
        let (_, s) = first_pace(&mut game, CREA, creature, skeleton, 1.0, None, &mut clo, D, &mut stats);
        ai.set_speeds(creature, s.unwrap());
        assert_eq!(ai.calm_speeds(creature).run, 0.0);
        call(&mut game, "SetActorValue", creature, sm, 150.0);
        let (c, s) = first_pace(&mut game, CREA, creature, skeleton, 1.0, None, &mut clo, D, &mut stats);
        assert_eq!(c.mult, 150.0);
        ai.set_speeds(creature, s.unwrap());
        assert_eq!(ai.calm_speeds(creature), clip_speeds(&caz, 1.0, 150.0, skeleton_share(skeleton), D));
        // The player and an actor without a record are left alone.
        let (own, s) = first_pace(&mut game, NPC, FormId::PLAYER_REF, "", 1.0, None, &mut lo, D, &mut stats);
        assert_eq!((own.kind, s), (PaceKind::Own, None));
    }

    /// A record without a Speed Mult of its own (0) moves at 100, as before; SetAV's 0
    /// stands. A reference outside the game's world reads its base's stats.
    #[test]
    fn a_blank_record_counts_as_100_but_set_av_0_stands() {
        let (mut game, [_, _, blank]) = speed_game();
        let sm = fnv_game::actor_values::SPEED_MULTIPLIER;
        assert_eq!(current_speed_mult(&mut game, BLANK, blank), 100.0);
        call(&mut game, "ModActorValue", blank, sm, 20.0);
        assert_eq!(current_speed_mult(&mut game, BLANK, blank), 120.0);
        call(&mut game, "SetActorValue", blank, sm, 0.0);
        assert_eq!(current_speed_mult(&mut game, BLANK, blank), 20.0);
        call(&mut game, "ModActorValue", blank, sm, -20.0);
        assert_eq!(current_speed_mult(&mut game, BLANK, blank), 0.0);
        let elsewhere = FormId(0xFF00_7000);
        assert_eq!(current_speed_mult(&mut game, CREA, elsewhere), 150.0);
        assert_eq!(current_speed_mult(&mut game, BLANK, elsewhere), 100.0);
    }
}
