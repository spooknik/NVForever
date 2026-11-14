//! Sleeping and waiting as scripts see and ask for them (docs/spec-notes/wait-menu.md
//! §5, §8 and §9). The Sleep / Wait menu itself is the engine's: it tells the game when
//! accepted hours start and stop passing ([`Rest::begin`], [`Rest::end`]) and takes what
//! scripts asked for ([`Rest::take_requests`]).
//!
//! Functions: `IsPCSleeping`, `IsTimePassing`, `SetPCSleepHours`, `WakeUpPC`,
//! `ShowSleepWaitMenu`. `GetPCSleepHours` has no opcode in the catalogue (no script or
//! condition of the game's plugins calls it), so it is not registered; its value is
//! [`Rest::sleep_hours`].
//!
//! Also here: who counts as a hostile actor near the player, and whose bed the player
//! may sleep in, for the menu's refusals (§9). Neither rule is sourced; each says so.

use crate::functions::{none, yes, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::Value;
use fnv_sim::FormId;

/// Hours accepted in the menu that are passing now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Passing {
    /// A sleep (the Sleep menu), not a wait.
    pub sleep: bool,
    /// The hours set in the menu.
    pub hours: u32,
}

/// What a script asked of the Sleep / Wait menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestRequest {
    /// `ShowSleepWaitMenu`: open the menu, as a Sleep menu or a Wait menu; `check`: only
    /// if the conditions for it hold (the refusals of wait-menu.md §9).
    Menu { sleep: bool, check: bool },
    /// `SetPCSleepHours`: the player sleeps this many hours.
    Sleep { hours: u32 },
    /// `WakeUpPC`: the sleep stops as if the player had pressed Cancel.
    Wake,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Rest {
    pub passing: Option<Passing>,
    /// For the engine's menu to take, in the order they were asked.
    pub requests: Vec<RestRequest>,
}

impl Rest {
    /// The hours accepted in the menu start passing.
    pub fn begin(&mut self, sleep: bool, hours: u32) {
        self.passing = Some(Passing { sleep, hours });
    }

    /// They have passed, or the player (or `WakeUpPC`) stopped them.
    pub fn end(&mut self) {
        self.passing = None;
    }

    /// `IsPCSleeping`: "sleeping in bed (not waiting)" [documented: GECK wiki
    /// IsPCSleeping]. [hypothesis: from the moment the hours are accepted until the
    /// menu closes; a bed's script reads it in MenuMode to learn that the player slept
    /// (FalloutNV.esm SCPT 0x000B5431), which fits, and nothing says when it starts.]
    pub fn sleeping(&self) -> bool {
        self.passing.is_some_and(|p| p.sleep)
    }

    /// `GetPCSleepHours`: "the number of hours the player has set to sleep from the
    /// menu", 0 when not sleeping [documented: GECK wiki GetPCSleepHours].
    pub fn sleep_hours(&self) -> u32 {
        self.passing.filter(|p| p.sleep).map_or(0, |p| p.hours)
    }

    /// `IsTimePassing`: the player "is sleeping, waiting, or traveling (i.e. time is
    /// passing while in menu mode)" [documented: GECK wiki IsTimePassing]. Fast travel
    /// is not built.
    pub fn time_passing(&self) -> bool {
        self.passing.is_some()
    }

    pub fn take_requests(&mut self) -> Vec<RestRequest> {
        std::mem::take(&mut self.requests)
    }
}

pub fn is_pc_sleeping(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    yes(g.rest.sleeping())
}

pub fn is_time_passing(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    yes(g.rest.time_passing())
}

/// `SetPCSleepHours hours`: "manually forces the player to sleep the specified number
/// of hours" [documented: GECK wiki SetPCSleepHours]. The game's scripts call it in game
/// mode after setting the GameHour global themselves [observed: FalloutNV.esm SCPT
/// 0x001446AE, 0x0013F374, 0x001672C8]. [hypothesis: the sleep starts as if the hours
/// had been accepted in the Sleep menu; a sleep already passing takes the new count.]
pub fn set_pc_sleep_hours(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let hours = a.int(0)?.max(0) as u32;
    g.rest.requests.push(RestRequest::Sleep { hours });
    none()
}

/// `WakeUpPC`: "manually closes the sleep menu (as if the player had pressed the Cancel
/// button)"; "will not work if the player is waiting" [documented: GECK wiki WakeUpPC].
/// The game's one caller calls it every frame, in MenuMode 1012 and in GameMode
/// [observed: FalloutNV.esm SCPT 0x0014BDAF], so outside a sleep it does nothing here.
pub fn wake_up_pc(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    if g.rest.sleeping() && !g.rest.requests.contains(&RestRequest::Wake) {
        g.rest.requests.push(RestRequest::Wake);
    }
    none()
}

/// `ShowSleepWaitMenu isSleep [checkPreconditions]`: 0 the Wait menu, 1 the Sleep menu;
/// with the second argument set the conditions for sleeping or waiting are checked, and
/// the first is ignored: it is always the Sleep menu [documented: GECK wiki
/// ShowSleepWaitMenu, its note on the original's behaviour].
pub fn show_sleep_wait_menu(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let check = a.num_or(1, 0.0) != 0.0;
    let sleep = check || a.num_or(0, 0.0) != 0.0;
    g.rest.requests.push(RestRequest::Menu { sleep, check });
    none()
}

/// Whether `actor` counts as a hostile actor for the menu's "enemies are nearby"
/// refusal. The original's rule is inside its program and not described by any source
/// read (wait-menu.md §9), and NVForever's actors do not fight on their own yet, so this
/// is what the records say an actor would do on seeing the player [engineering choice]:
/// a living, enabled actor that is not the player's teammate, unconscious or restrained,
/// by its Aggression (0 Unaggressive, 1 Aggressive, 2 Very Aggressive, 3 Frenzied):
/// "Enemy actors are attacked on sight by Aggressive and Very Aggressive actors",
/// "Neutrals will be attacked by Very Aggressive actors", allies and friends are not
/// attacked [documented: GECK wiki GetFactionCombatReaction], with the factions combined
/// as `GetFactionRelation` does; a Frenzied actor attacks anyone [hypothesis].
///
/// An actor's factions and aggression are its template's where the template flags say so
/// (docs/spec-notes/factions.md §2, §3). Most hostile actors are Very Aggressive and
/// neutral to the player by their factions (§5 there), a Fiend for one.
///
/// The rule is [`Game::would_attack`]'s with the player as the other, which the
/// detection states use too (`crate::awareness`).
pub fn hostile_to_player(g: &mut Game, actor: FormId) -> bool {
    // The hours waited may have ended crimes (`crate::crime`, #2467).
    g.expire_crimes();
    let hostile = g.would_attack(actor, FormId::PLAYER_REF);
    tracing::debug!(%actor, hostile, "hostile to the player?");
    hostile
}

/// Whether the player may not sleep in `bed` because of who owns it, and the owner. The
/// original has a refusal text for an owned bed (`sNoSleepInOwnedBed`) and no source read
/// gives its rule (wait-menu.md §9). [hypothesis: the bed reference's own owner counts,
/// as `IsOwner` reads it, not the cell's: an actor other than the player, or a faction
/// the player is not in.]
pub fn bed_owned_by_another(g: &mut Game, bed: FormId) -> Option<FormId> {
    let owner = g.world.get(bed).ok()?.extra.owner?;
    let mine = owner == FormId::PLAYER_BASE || crate::functions::actor::faction_rank(g, FormId::PLAYER_REF, owner) >= 0;
    (!mine).then_some(owner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sleep_is_not_a_wait() {
        let mut r = Rest::default();
        assert!(!r.sleeping() && !r.time_passing() && r.sleep_hours() == 0);
        r.begin(false, 5);
        assert!(!r.sleeping() && r.time_passing());
        assert_eq!(r.sleep_hours(), 0, "waiting is not sleeping");
        r.end();
        r.begin(true, 8);
        assert!(r.sleeping() && r.time_passing());
        assert_eq!(r.sleep_hours(), 8);
        r.end();
        assert!(!r.sleeping() && !r.time_passing() && r.sleep_hours() == 0);
    }

    #[test]
    fn requests_are_taken_once_in_order() {
        let mut r = Rest::default();
        r.requests.push(RestRequest::Menu { sleep: true, check: false });
        r.requests.push(RestRequest::Sleep { hours: 3 });
        assert_eq!(r.take_requests(), vec![RestRequest::Menu { sleep: true, check: false }, RestRequest::Sleep { hours: 3 }]);
        assert!(r.take_requests().is_empty());
    }
}
