//! Vanilla functions only Fallout 3's scripts call: registered in the catalogue (GECK1,
//! opcode recorded), never called by the New Vegas masters, first run by Tale of Two
//! Wastelands (docs/handoff/ttw-fo3-script-audit-2026-11-02.md §1). Behaviour from the
//! catalogue (`summary`/`notes`) unless tagged otherwise.

use super::{actor, none, yes, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::{ArgValue, Value};
use fnv_sim::FormId;

// ---- Actors -----------------------------------------------------------------------------

/// `actor.ModDisposition target change`: the caller's disposition towards `target`
/// changes by `change` [documented: catalogue summary]. The change starts from what
/// GetDisposition reads now and is kept as SetDisposition keeps its value; the result
/// stays within 0–100, GetDisposition's range [hypothesis].
pub fn mod_disposition(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let me = ctx.subject()?;
    let target = a.form(0)?;
    let change = a.num(1)?;
    let now = actor::get_disposition(g, ctx, &Args(&[ArgValue::Value(Value::Ref(target.0))]))?.num();
    g.extra.entry(me).or_default().dispositions.insert(target, (now + change).clamp(0.0, 100.0));
    none()
}

/// `GetPCIsRace race`: the player's race is `race`, as GetIsRace on the player
/// [documented: catalogue summary, "the player-only version of GetIsRace"].
pub fn get_pc_is_race(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    yes(g.actor_race(FormId::PLAYER_REF) == Some(a.form(0)?))
}

/// `actor.GetForceSneak`: SetForceSneak's flag on the caller [documented: catalogue
/// summary].
pub fn get_force_sneak(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(g.extra.get(&r).is_some_and(|e| e.force_sneak))
}

/// `actor.SetMerchantContainer container`: the caller's goods are in `container` from
/// now on, in place of its reference's merchant container (XMRC), for the barter menu
/// and for actors that buy from it ([`Game::merchant_container`]). Refused for a
/// container that is not persistent [documented: catalogue notes].
pub fn set_merchant_container(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let me = ctx.subject()?;
    let c = a.form(0)?;
    if !g.world.placed.get(c).is_some_and(|p| p.in_persistent_group) {
        return Err(format!("{} is not a persistent reference: SetMerchantContainer refused", g.data.name(c)));
    }
    g.extra.entry(me).or_default().merchant_container = Some(c);
    none()
}

/// `actor.MatchRace source`: the caller takes the race `source` has now, drawn again
/// with the new race's default face textures [documented: catalogue summary and notes]
/// (`crate::appearance`).
pub fn match_race(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let me = ctx.subject()?;
    let race = g.actor_race(a.form(0)?).ok_or("the source actor has no race")?;
    g.change_race(me, race);
    none()
}

/// `ClearActorsFactionsPlayerEnemyFlag actor`: the player is no longer an enemy of any
/// crime-tracking faction `actor` belongs to [documented: catalogue summary], as
/// ClearFactionPlayerEnemyFlag on each.
pub fn clear_actors_factions_player_enemy_flag(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = a.form(0)?;
    let tracks = |g: &Game, f: FormId| g.data.faction_flags.get(&f).is_some_and(|x| x & crate::crime::faction_flags::TRACK_CRIME != 0);
    for f in actor::factions_of(g, r) {
        if tracks(g, f) {
            g.player.enemy_of.remove(&f);
        }
    }
    actor::forget_relations(g);
    none()
}

// ---- The player -------------------------------------------------------------------------

/// `GetPCSleepHours`: the hours of the sleep the player chose in the menu, 0 while
/// awake [documented: catalogue summary] (`crate::rest`).
pub fn get_pc_sleep_hours(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    Ok(Value::Num(g.rest.sleep_hours() as f64))
}

/// `SetPCYoung flag`: the player counts as young (Fallout 3's childhood in Vault 101):
/// only the race's default hair, no face attachments [documented: catalogue summary].
/// Kept for the engine to draw by ([`crate::game::PlayerState::young`]).
pub fn set_pc_young(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.young = a.num_or(0, 1.0) != 0.0;
    none()
}

/// `SetPCToddler flag`: the player moves with the toddler's walk animations (1) or
/// the normal ones (0) [documented: catalogue summary]. Kept for the engine
/// ([`crate::game::PlayerState::toddler`]).
pub fn set_pc_toddler(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.toddler = a.num_or(0, 1.0) != 0.0;
    none()
}

/// `SetNoActivationSound sound`: the sound played when an activation is refused is
/// `sound` (an editor ID) in place of the default [documented: catalogue summary]. Kept
/// ([`crate::game::PlayerState::no_activation_sound`]).
pub fn set_no_activation_sound(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let sound = match a.peek_str(0) {
        Some(s) => g.data.by_edid(s).ok_or_else(|| format!("no form has the editor ID {s}"))?,
        None => a.form(0)?,
    };
    g.player.no_activation_sound = Some(sound);
    none()
}

/// `ClearNoActivationSound`: the default sound again [documented: catalogue summary].
pub fn clear_no_activation_sound(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.player.no_activation_sound = None;
    none()
}

/// `SetLocationSpecificLoadScreensOnly flag`: with 1, a load screen shows only when one
/// is tied to the place being loaded [documented: catalogue summary]. Kept for the
/// engine's loading screen ([`crate::game::PlayerState::location_load_screens_only`]).
pub fn set_location_specific_load_screens_only(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.player.location_load_screens_only = a.num(0)? != 0.0;
    none()
}

/// `SetLastExtDoorActivated door`: `door` counts as the last exterior door the player
/// used, which chooses the worldspace the Pip-Boy's map shows [documented: catalogue
/// summary and notes]; GetPCLastExteriorDoor reads it (`crate::nvse_jip3`).
pub fn set_last_ext_door_activated(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.gaps.jip3.last_exterior_door = Some(a.form(0)?);
    none()
}

impl Game<'_> {
    /// The container that holds `actor`'s goods: SetMerchantContainer's, else its
    /// reference's (XMRC).
    pub fn merchant_container(&self, actor: FormId) -> Option<FormId> {
        self.extra.get(&actor).and_then(|e| e.merchant_container).or_else(|| self.world.placed.get(actor).and_then(|p| p.merchant_container))
    }
}
