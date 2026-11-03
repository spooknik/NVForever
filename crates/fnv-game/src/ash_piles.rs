//! Ash and goo piles (docs/spec-notes/ash-piles.md): what an energy weapon's critical
//! kill leaves of the body.
//!
//! - A weapon's critical effect (WEAP CRDT: an actor effect and the On Death flag) is
//!   applied to the one hit by a critical hit; with On Death only when that hit kills
//!   ([`Game::apply_crit_effect`]; §1).
//! - The shipped critical effects that disintegrate (the laser's and the alien
//!   weapons' Disintegration, the plasma weapons' Gooification) are Script effects whose
//!   scripts do the rest: SetCriticalStage …Start, the effect shaders, then
//!   `AttachAshPile` and SetCriticalStage …End, which culls the body
//!   (`functions::objects::set_critical_stage`; §2).
//! - `AttachAshPile [type]` places the pile activator at the actor ([`Game::attach_ash_pile`]);
//!   activating the pile opens the dead actor's inventory (the engine's activation,
//!   [`Game::ash_pile_source`]; §3).

use crate::game::Game;
use fnv_sim::FormId;

/// `DefaultAshPile1` "Ash Pile" (ACTI, `Effects\AshPile01.NIF`) [observed: FalloutNV.esm
/// 0000001B]; found by Editor ID first.
pub const ASH_PILE: FormId = FormId(0x0000_001B);
/// `DefaultAshPile2` "Goo Pile" (ACTI, `Effects\GooPile01.NIF`) [observed: FalloutNV.esm
/// 00000022].
pub const GOO_PILE: FormId = FormId(0x0000_0022);

/// What a critical effect did ([`Game::apply_crit_effect`]).
#[derive(Debug, Clone, PartialEq)]
pub enum CritEffect {
    /// Its effects started (their ids).
    Applied(Vec<u32>),
    /// An On Death effect on a hit that did not kill.
    NotKilled,
    /// The target was already dead, cannot be harmed, or is the player.
    NotApplied,
}

impl Game<'_> {
    /// The pile base `AttachAshPile type` places: 2 the goo pile, anything else the ash
    /// pile [documented: GECK wiki AttachAshPile, 1 normal ash pile, 2 plasma goo pile;
    /// hypothesis: other numbers give the ash pile].
    pub fn ash_pile_base(&self, kind: i64) -> FormId {
        let (edid, id) = if kind == 2 { ("defaultashpile2", GOO_PILE) } else { ("defaultashpile1", ASH_PILE) };
        self.data.edids.get(edid).copied().unwrap_or(id)
    }

    /// `actor.AttachAshPile [type]`: the pile is placed where the actor is, turned as it
    /// is (upright, its heading), and linked to it both ways (`RefExtra::ash_pile`,
    /// `ash_pile_source`) [documented: GECK wiki AttachAshPile "attaches an ash pile to
    /// the calling reference"; JIP's GetAshPileSource returns the actor a pile came from;
    /// hypothesis: the place and the angles]. An actor that has a pile gets no second one
    /// [hypothesis]. When the player killed the actor, Disintegrations counts one
    /// [hypothesis: the statistic's name; misc-stats.md]. Returns the pile.
    pub fn attach_ash_pile(&mut self, actor: FormId, kind: i64) -> Result<FormId, String> {
        if let Some(p) = self.extra.get(&actor).and_then(|e| e.ash_pile) {
            return Ok(p);
        }
        let base = self.ash_pile_base(kind);
        let s = self.world.get(actor).map_err(|e| e.to_string())?;
        let (cell, pos, rot) = (s.cell, s.pos, [0.0, 0.0, s.rot[2]]);
        let pile = self.create_at(base, cell, pos, rot)?;
        self.extra.entry(actor).or_default().ash_pile = Some(pile);
        self.extra.entry(pile).or_default().ash_pile_source = Some(actor);
        let by_player = self.extra.get(&actor).is_some_and(|e| e.killer == Some(FormId::PLAYER_REF));
        if by_player {
            self.count_stat(crate::misc_stats::DISINTEGRATIONS, 1.0);
        }
        tracing::info!("ash pile: {pile} ({}) replaces the body of {actor} at [{:.0}, {:.0}, {:.0}]", self.data.name(base), pos[0], pos[1], pos[2]);
        Ok(pile)
    }

    /// The actor a pile came from (JIP's GetAshPileSource): activating the pile searches
    /// that actor's inventory.
    pub fn ash_pile_source(&self, pile: FormId) -> Option<FormId> {
        self.extra.get(&pile).and_then(|e| e.ash_pile_source)
    }

    /// A weapon's critical effect `spell` on the one a critical hit struck, after the
    /// hit's damage: with On Death (`on_death`) only when the hit killed (`died`), else on
    /// every critical hit [documented: GECK wiki Weapons, "Crit Effect: The Actor Effect
    /// that is applied to the target on a critical hit … On Death: If checked, the
    /// Critical Effect is only applied when the target is killed"]. An effect for a
    /// living target is not given to a corpse, and the player takes none
    /// [hypothesis: the player's death camera has no culled body; ash-piles.md §1].
    pub fn apply_crit_effect(&mut self, target: FormId, spell: FormId, on_death: bool, died: bool, caster: Option<FormId>) -> CritEffect {
        if target == FormId::PLAYER_REF || self.is_immune(target) {
            return CritEffect::NotApplied;
        }
        if on_death && !died {
            return CritEffect::NotKilled;
        }
        if !on_death && self.is_dead(target) && !died {
            return CritEffect::NotApplied;
        }
        CritEffect::Applied(self.apply_magic_item(target, spell, caster, false))
    }

    /// ResurrectActor of an actor that left a pile: the pile is removed [hypothesis].
    pub(crate) fn remove_ash_pile(&mut self, actor: FormId) {
        let Some(pile) = self.extra.get_mut(&actor).and_then(|e| e.ash_pile.take()) else { return };
        let _ = self.world.set_deleted(pile, true);
        tracing::info!("ash pile: {pile} removed, {actor} is alive again");
    }
}
