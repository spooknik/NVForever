//! AI state functions over the package model (`crate::ai`) and the detection states
//! (`crate::awareness`): ResetAI, GetCurrentAIPackage, GetCurrentAIProcedure, and the
//! forced head tracking of Look and StopLook (docs/design/script-stubs.md §3).

use super::{none, Args, Ctx};
use crate::ai::{AiPhase, SandboxPhase};
use crate::awareness::State;
use crate::functions::furniture::bed;
use crate::functions::senses::Look;
use crate::game::Game;
use crate::package::PackageType;
use fnv_script::vm::Value;
use fnv_sim::FormId;

/// `actor.ResetAI`: clears the actor's current AI behaviour; its packages are evaluated
/// afresh, and packages added with AddScriptPackage stop and are removed [documented:
/// GECK wiki ResetAI]. Here: the script package goes, the current package's "goal met"
/// and the packages it could not reach are forgotten, a Look is let go, and the actor
/// chooses again (`Game::evaluate_package`). Combat and pathing, which the page says
/// are re-evaluated too, are the engine's and the combat AI's: they are not touched
/// here [engineering choice: combat states belong to `crate::awareness` and the combat
/// workstream; the walker takes the new package on its next step]. In the original an
/// NCR trooper given ResetAI in a fight left combat for one sample (2 game loops, back on
/// his FOLLOW package) and was in combat again with a new combat package 2 loops later,
/// a Fiend his target within 0.25 s [observed: AI observer campaign `fights_round`
/// 2026-10-19, script-function-coverage.md §3.3], so leaving combat alone looks the same.
///
/// The AI is also asked to take the package up afresh (`RefExtra::ai_reset`), even when
/// the same package is chosen again: a seated actor gets up and its package begins again
/// [design, issue #2119, docs/design/actor-behaviour.md §5]. What the original showed:
/// RingoRef seated on his sandbox (procedure 48) read GET_UP (21) one loop after ResetAI,
/// then TRAVEL [observed: owner-decision-captures.md od_06]. Whether a sleeping actor
/// wakes is not known (od_06 (c): he never lay down).
pub fn reset_ai(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let e = g.extra.entry(r).or_default();
    e.script_package = None;
    e.package_goal_met = false;
    e.package_blocked.clear();
    e.ai_reset = true;
    g.senses.looks.remove(&r);
    // A weapon use (UseWeapon) stops too [design: the page's "current AI behaviour";
    // the shipped scripts call ResetAI to end one, FalloutNV.esm SCPT 0x000E07CE].
    g.drop_weapon_use(r);
    if g.runs_packages() {
        g.evaluate_package(r);
    } else if let Some(e) = g.extra.get_mut(&r) {
        // The runner's inputs choose packages: the choice is asked for again.
        e.current_package = None;
    }
    none()
}

/// The GetCurrentAIPackage code of a package type [documented: GECK wiki
/// GetCurrentAIPackage's table: 1 FOLLOW, 2 ESCORT, 3 EAT, 4 SLEEP, 13 WANDER, 14
/// TRAVEL, 15 ACCOMPANY, 16 USEITEMAT, 17 AMBUSH, 18 FLEE_NON_COMBAT, 33 USE_WEAPON, 34
/// DIALOGUE, 35 GUARD, 36 SANDBOX, 37 PATROL; hypothesis: Find is 0 EXPLORE, the table
/// having no FIND, and a Dialogue package is the second DIALOGUE (34), 6 being the
/// conversation itself]. The original agreed for Sandbox 36, Dialogue 34, Travel 14 and
/// Patrol 37 [observed: function probes `fp7_packages_locks` 2026-10-17, each actor's
/// current package found with GetIsCurrentPackage].
pub fn package_code(t: PackageType) -> i32 {
    match t {
        PackageType::Find => 0,
        PackageType::Follow => 1,
        PackageType::Escort => 2,
        PackageType::Eat => 3,
        PackageType::Sleep => 4,
        PackageType::Wander => 13,
        PackageType::Travel => 14,
        PackageType::Accompany => 15,
        PackageType::UseItemAt => 16,
        PackageType::Ambush => 17,
        PackageType::FleeNotCombat => 18,
        PackageType::UseWeapon => 33,
        PackageType::Dialogue => 34,
        PackageType::Guard => 35,
        PackageType::Sandbox => 36,
        PackageType::Patrol => 37,
        PackageType::Other(_) => -1,
    }
}

/// What the actor does over its package: in a conversation (with the player, or a
/// conversation between two actors), in combat, searching, or the package.
enum Doing {
    Talking,
    /// In Combat, or Lost (searching for a target it fought): scripts read a plain fight
    /// [observed: an actor of the original fighting the player kept procedure COMBAT and
    /// package type COMBAT while GetDetected player read 0, and SEARCH 36 and
    /// SEARCH_FOR_ATTACKER 29 never appeared in about 480,000 samples,
    /// npc-behaviour-compare.md §11; design, #2121: our Lost searches underneath].
    Fighting,
    /// Fleeing from a fight (`combat_group::Flight`, in any combat state).
    Fleeing,
    /// Alert (noticed, never fought this time): no capture shows what the original reads
    /// then (npc-behaviour-compare.md §11), so it keeps SEARCH [design, #2121: left as it
    /// was until a capture shows it].
    Searching,
    /// Warning the player, in Normal (`Game::warning_to`): the game runs a Guard package
    /// of its own meanwhile [observed: a young gecko's package read a form made at run
    /// time (FF…), GetCurrentAIPackage 35 GUARD and GetCurrentAIProcedure 44 GUARD from
    /// its first warning until it fought; capture `warnings`, young_gecko, 2026-10-24].
    Warning,
    Package(Option<PackageType>),
}

fn doing(g: &mut Game, r: FormId) -> Doing {
    if g.dialogue.as_ref().is_some_and(|d| d.actor == r) {
        return Doing::Talking;
    }
    // Both sides of a conversation between actors talk [hypothesis: the IDLE tree's
    // branch for such conversations (NPCtoNPC) lies under a root that wants the dialogue
    // procedure or the dialogue menu, so it is reached only if they do;
    // docs/spec-notes/idles.md §12].
    if g.speech.iter().any(|s| s.conversation.is_some() && (s.speaker == r || s.target == Some(r))) {
        return Doing::Talking;
    }
    if g.awareness.state(r).in_combat() && g.awareness.tactics.flights.contains_key(&r) {
        return Doing::Fleeing;
    }
    match g.awareness.state(r) {
        State::Combat | State::Lost => return Doing::Fighting,
        State::Alert => return Doing::Searching,
        State::Normal => {}
    }
    if g.warning_to(r).is_some() {
        return Doing::Warning;
    }
    let p = g.current_package(r);
    Doing::Package(p.and_then(|p| g.package_def(&p)).map(|d| d.ty))
}

/// `actor.GetCurrentAIPackage`: the kind of package the actor runs, by the codes of
/// [`package_code`]; 5 COMBAT in combat (Combat and Lost; fleeing from it too), 29
/// SEARCH_FOR_ATTACKER in Alert, 6 DIALOGUE in a conversation with the player
/// [hypothesis: the game's combat, search and conversation run as packages of their own, as the table's
/// COMBAT, SEARCH_FOR_ATTACKER and DIALOGUE suggest]; 0 with no package [observed:
/// Trudy and Joe Cobb, none of whose packages was current, read 0, `fp7_packages_locks`].
pub fn get_current_ai_package(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let code = match doing(g, r) {
        Doing::Talking => 6,
        Doing::Fighting | Doing::Fleeing => 5,
        Doing::Searching => 29,
        Doing::Warning => 35,
        Doing::Package(t) => t.map_or(0, package_code),
    };
    Ok(Value::Num(code as f64))
}

/// `actor.GetOffersServicesNow`: 1 while the actor's current package offers its
/// services (`crate::services`, docs/spec-notes/vendor-services.md §3).
pub fn get_offers_services_now(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    Ok(Value::Num(if g.offers_services_now(r) { 1.0 } else { 0.0 }))
}

/// `actor.GetCurrentAIProcedure`: what the actor is doing within its package, by the
/// codes of the GECK wiki's table [documented: GECK wiki GetCurrentAIProcedure: 0
/// TRAVEL, 2 ACQUIRE, 3 WAIT, 4 DIALOGUE, 7 WANDER, 8 SLEEP, 10 EAT, 11 FOLLOW, 12
/// ESCORT, 13 COMBAT, 16 FLEE, 17 DONE, 21 GET_UP, 27 ACCOMPANY, 28 USE_ITEM_AT, 30
/// WAIT_AMBUSH, 34 FLEE_NON_COMBAT, 36 SEARCH, 41 PATROL, 42 USE_WEAPON, 43
/// DIALOGUE_ACTIVATE, 44 GUARD, 45 SANDBOX, 46 USE_IDLE_MARKER, 48 SITTING; "a single
/// package can contain several possible procedures"]. Chosen here:
///
/// - a conversation with the player or between two actors 4, combat 13 (Combat and Lost),
///   Alert 36;
/// - warning the player (`Game::warning_to`) 44 GUARD [observed: [`Doing::Warning`]];
/// - fleeing from a fight 16 FLEE [documented: the wiki's table names 16 FLEE;
///   hypothesis: it is the procedure of the combat AI's flight, as the IDLE tree reads it:
///   `CowerIdles` (FalloutNV.esm 0x0006CB4A) and `3rdPCrouchedHit` want 16 or 34 with
///   IsMoving 0, and the creatures' combat idles 13 or 16];
/// - else what the actor does in its package ([`package_procedure`]);
/// - 0 with no package [observed: Trudy and Joe Cobb, `fp7_packages_locks`].
pub fn get_current_ai_procedure(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let furniture = g.furniture_use(r);
    let (goal_met, phase) = g.extra.get(&r).map_or((false, None), |e| (e.package_goal_met, e.ai_phase));
    let code = match doing(g, r) {
        Doing::Talking => 4,
        Doing::Fighting => 13,
        Doing::Fleeing => 16,
        Doing::Searching => 36,
        Doing::Warning => 44,
        Doing::Package(None) => 0,
        Doing::Package(Some(t)) => package_procedure(t, phase, furniture.map(|u| (u.state, bed(&u))), goal_met),
    };
    Ok(Value::Num(code as f64))
}

/// The procedure of an actor in a package of type `t` (see [`get_current_ai_procedure`]),
/// from what the AI layer has it do (`phase`, [`AiPhase`]; `None` when that layer does
/// not run it), the furniture it uses (its state, 3 on it and 4 getting off, and whether
/// it is a bed) and whether the package met its goal. From the AI observer's samples of
/// the original, about 430 000 actor samples over the Goodsprings, Strip, casino,
/// package-kind and owner-decision campaigns (docs/spec-notes/packages.md §4, procedure codes)
/// [observed]:
///
/// - getting off furniture 21 GET_UP;
/// - a package that ends at its goal (Travel, Patrol, Dialogue, Use Item At, Flee Not
///   Combat) 17 DONE once there, seated or in bed too [observed: Travel 17 standing,
///   seated and asleep, 0 on the way]; Find, Escort, Ambush and Use Weapon too
///   [hypothesis: their goals end as those do; no sample reached one]. Not Sandbox,
///   Wander, Guard, Follow, Accompany, Eat or Sleep, which never read 17 [observed];
/// - Sandbox, by what it does there: on the way to the area or to something to use, or
///   standing between two things, 0; walking to a spot of the area or standing at it 7
///   WANDER; at an idle marker 46; seated 48; eating 10; asleep 8; up from furniture 21
///   until it does the next thing (walking away too); an actor the AI does not run (out
///   of the high process) 45 [observed: 45 only for an actor out of the loaded cells,
///   Chet; the others by what each was seen doing, sitting, sleeping and moving read
///   with GetSitting, GetSleeping and IsMoving];
/// - on the way to its location, 0 for Travel, Eat, Use Item At, Use Weapon, Dialogue and
///   Sandbox packages [observed]; the others keep their own procedure walking (Patrol 41,
///   Follow 11, Guard 44, Wander 7, Escort 12) [observed];
/// - Sleep 8 (in bed and on the way to it) [observed: 8 while moving outnumbered 0
///   three to one];
/// - else the package type's main procedure ([`procedure_code`]).
pub fn package_procedure(t: PackageType, phase: Option<AiPhase>, furniture: Option<(u8, bool)>, goal_met: bool) -> i32 {
    if furniture.is_some_and(|(state, _)| state == 4) {
        return 21;
    }
    if goal_met && t.has_done_state() {
        return 17;
    }
    let seated = furniture.filter(|(state, _)| *state == 3);
    if t == PackageType::Sandbox {
        return match phase {
            Some(AiPhase::Sandbox(p)) => match p {
                SandboxPhase::Standing | SandboxPhase::ToObject => 0,
                SandboxPhase::Wandering => 7,
                SandboxPhase::AtIdleMarker => 46,
                SandboxPhase::Sitting => 48,
                SandboxPhase::Eating => 10,
                SandboxPhase::Sleeping => 8,
                SandboxPhase::GotUp => 21,
            },
            Some(AiPhase::Travelling | AiPhase::There) => 0,
            None => match seated {
                Some((_, true)) => 8,
                Some((_, false)) => 48,
                None => 45,
            },
        };
    }
    if phase == Some(AiPhase::Travelling) && walks_as_travel(t) {
        return 0;
    }
    procedure_code(t)
}

/// Package types whose actor reads 0 TRAVEL on its way to the package's location (see
/// [`package_procedure`]).
fn walks_as_travel(t: PackageType) -> bool {
    matches!(t, PackageType::Travel | PackageType::Eat | PackageType::UseItemAt | PackageType::UseWeapon | PackageType::Dialogue)
}

/// The main procedure of a package type (see [`package_procedure`]): Travel 0, Find 2,
/// Wander 7, Sleep 8, Eat 10, Follow 11, Escort 12, Accompany 27, Use Item At 28, Ambush
/// 30, Flee Not Combat 34, Patrol 41, Use Weapon 42, Dialogue 43, Guard 44, Sandbox 45
/// [observed for all but Find and Ambush, AI observer samples; Dialogue 43 also Sunny
/// Smiles, captures `fa_batch2_loaded_npcs` and `fp7_packages_locks`; hypothesis: Find 2
/// and Ambush 30, the wiki's ACQUIRE and WAIT_AMBUSH].
pub fn procedure_code(t: PackageType) -> i32 {
    match t {
        PackageType::Travel => 0,
        PackageType::Sleep => 8,
        PackageType::Find => 2,
        PackageType::Dialogue => 43,
        PackageType::Wander => 7,
        PackageType::Eat => 10,
        PackageType::Follow => 11,
        PackageType::Escort => 12,
        PackageType::Accompany => 27,
        PackageType::UseItemAt => 28,
        PackageType::Ambush => 30,
        PackageType::FleeNotCombat => 34,
        PackageType::Patrol => 41,
        PackageType::UseWeapon => 42,
        PackageType::Guard => 44,
        PackageType::Sandbox => 45,
        PackageType::Other(_) => -1,
    }
}

/// `actor.Look target [rotateBody]`: the actor's head tracks the target, over all
/// other head tracking, until StopLook or the target leaves the loaded area; with
/// `rotateBody` 1 the whole body turns to it [documented: GECK wiki Look]. Kept here
/// ([`Game::script_look`]); the engine turns the head (`nvforever` `script_look.rs`),
/// and with `rotateBody` the body too ([`Game::body_looks`]: `fnv_ai` for an actor it
/// runs, `script_look.rs` for one standing still).
pub fn look(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let target = a.form(0)?;
    let rotate_body = a.num_or(1, 0.0) != 0.0;
    g.senses.looks.insert(r, Look { target, rotate_body });
    none()
}

/// `actor.StopLook`: the forced head tracking of Look ends; normal head tracking
/// resumes [documented: GECK wiki StopLook].
pub fn stop_look(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.senses.looks.remove(&r);
    none()
}

/// `actor.SetPackDuration minutes`: the actor's current package runs for that many game
/// minutes [documented: GECK wiki SetPackDuration, "useful … for less than an hour"].
/// Counted from the call [hypothesis: from the call rather than from the package's start;
/// the two agree for the shipped calls in begin scripts, and the Hidden Valley restroom
/// trigger's `enteringActor.SetPackDuration 1` (`SCPT 0x000CE229`) only makes sense
/// from the call, [observed: decoded scripts]]. When the time is up the package ends
/// ([`Game::end_timed_packages`]). Without a current package nothing happens.
pub fn set_pack_duration(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let minutes = a.num(0)?.max(0.0);
    let now = g.world.calendar().map(|c| c.days_passed as f64).unwrap_or(0.0);
    if let Some(p) = g.extra.get(&r).and_then(|e| e.current_package) {
        let e = g.extra.entry(r).or_default();
        e.package_ends = Some((p, now + minutes / (24.0 * 60.0)));
        e.package_ends_by_schedule = false;
    }
    none()
}

/// `actor.SetActorsAI flag`: 0 stops the actor's AI, any other value starts it again;
/// its scripts still run, but it does not process packages, detection, Havok or
/// animation [documented: GECK wiki SetActorsAI]. Here: the periodic package choice
/// leaves it out (`crate::ai`), the AI that carries packages out does not step it
/// (`fnv_ai` `Ai::step_actor`: no walk, no sandbox), and the engine does not let it
/// detect (`nvforever` `detection.rs`). Its animation and collision are not stopped
/// [engineering choice: no engine switch for them yet; the actor stands in its idle].
pub fn set_actors_ai(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    g.extra.entry(r).or_default().ai_off = a.num(0)? == 0.0;
    none()
}
