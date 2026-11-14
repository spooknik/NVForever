//! AgeRace and SexChange: an actor's race or sex changed by a script
//! (docs/spec-notes/appearance-changes.md).
//!
//! - **AgeRace n** moves the actor `n` steps along its race's age links: each step up to
//!   the race's older race (RACE ONAM), each step down to its younger one (YNAM); a step
//!   past the end of the chain is not taken (it clamps at the oldest or youngest)
//!   [documented: GECK wiki AgeRace, catalogue notes; xEdit RACE "Older", "Younger"]. A
//!   race without links (Ghoul) does not change [documented: "only races with linked age
//!   variants are affected"]. The masters' links are symmetric for the four playable
//!   races (adult ↔ child, adult ↔ old) [observed: FalloutNV.esm RACE records], so the
//!   opening's `player.AgeRace -1` then `player.AgeRace 1` (VCG00 stage 5) leaves the
//!   player's race as it was [observed: the stage's result script].
//! - **SexChange [sex] [reset]** gives the actor that sex, or the other one without a
//!   sex; no calling reference is the player [documented: GECK wiki SexChange]. With
//!   reset (default 0) and only on the player, and only when the sex really changes, the
//!   face becomes the race's first starting face for the new sex [documented: catalogue
//!   notes, "the default preset for that race and gender"; hypothesis: the first preset
//!   is the default]; the engine picks it (`AppearanceChange::reset`), as the character
//!   creator does when its sex is changed (the opening's VCG01 asks the player's sex with
//!   reset 1 [observed: VCG01SCRIPT]).
//! - **Kept**: on an NPC the change is kept on the reference (`RefExtra::race`,
//!   `RefExtra::female`), so saves keep it; on the player in the character creator's look
//!   (`PlayerState::look`), or before the creator in `PlayerState::race` and
//!   `PlayerState::female`. GetIsRace, GetIsSex, GetPCIsSex, IsChild and the race's
//!   abilities read the changed values.
//! - **Drawn anew**: each change is told to the engine ([`Game::take_appearance_changes`]),
//!   which draws the actor again from its record with the new race and sex: the new
//!   race's head, body and default face textures [documented: AgeRace notes, "the actor
//!   takes the default face textures of the new race"], and the race's default hair when
//!   its hair is not one the new race or sex may have [documented: SexChange notes, "an
//!   NPC whose hair is invalid for the new sex gets the default hair"; AgeRace notes,
//!   "invalid hair falls back to default"]. The player's hair is kept [hypothesis: the
//!   original restores it on the way back, "restored on return for the player only"].
//! - **MatchFaceGeometry source percent** (Fallout 3's; Tale of Two Wastelands calls it
//!   on Dad in Vault101ExitScript and CG04 stage 65, `MatchFaceGeometry player
//!   CGMatchFace`) morphs the actor's face geometry `percent` (0–100) of the way from
//!   its current face to `source`'s; 100 is the same face; the face texture is not
//!   changed [documented: GECK wiki MatchFaceGeometry and its notes]. The geometry is
//!   the FaceGen shape coefficients, symmetric (FGGS) and asymmetric (FGGA)
//!   [documented: xEdit wbFaceGen]; each moves in a straight line towards the source's
//!   [hypothesis: "a 50% morph from his current face" read as a weighted mean of the
//!   coefficients]. An NPC keeps its calls in order ([`FaceMatch`], `RefExtra::
//!   face_matches`, saved) and is drawn again; the engine applies them to the record's
//!   coefficients as it draws the head (`nvforever` actors.rs). A source the game holds
//!   the face of (the player, once the character creator has given a look) has its
//!   coefficients kept with the call, so the face matched is the one it had then; any
//!   other source's is read from its record when drawn [design]. On the player the
//!   creator's look itself is changed, which needs both faces known.

use crate::game::Game;
use fnv_sim::FormId;
use serde::{Deserialize, Serialize};

/// A change for the engine: draw the actor again (module notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AppearanceChange {
    pub actor: FormId,
    /// SexChange's reset on the player: its face becomes the race's first starting face
    /// for the new sex.
    pub reset: bool,
}

/// One MatchFaceGeometry call (module notes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaceMatch {
    /// The actor whose face is matched.
    pub source: FormId,
    /// How far, 0–100.
    pub percent: f32,
    /// The source's FGGS and FGGA when the game held them at the call; empty: the
    /// engine reads them from the source's record.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub fggs: Vec<f32>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub fgga: Vec<f32>,
}

/// `current` moved `percent` (0–100) of the way to `toward`, coefficient by coefficient;
/// a coefficient one side lacks counts as 0 (module notes).
pub fn blend_coefficients(current: &[f64], toward: &[f64], percent: f64) -> Vec<f64> {
    let t = (percent / 100.0).clamp(0.0, 1.0);
    let at = |v: &[f64], i: usize| v.get(i).copied().unwrap_or(0.0);
    (0..current.len().max(toward.len())).map(|i| at(current, i) + (at(toward, i) - at(current, i)) * t).collect()
}

/// The longest walk along a race's age links (a guard against a looping chain).
const MAX_STEPS: i32 = 16;

impl Game<'_> {
    /// The actor's race now: a script's change, the character creator's choice for the
    /// player, else the record's (template flag "Traits").
    pub fn actor_race(&self, r: FormId) -> Option<FormId> {
        if r == FormId::PLAYER_REF {
            if let Some(l) = self.player.look.as_ref() {
                return Some(l.race);
            }
            if let Some(race) = self.player.race {
                return Some(race);
            }
        } else if let Some(race) = self.itr_race(r).or_else(|| self.extra.get(&r).and_then(|e| e.race)) {
            // ITR's SetRaceAlt first (`crate::nvse_itr2`), then a script's change.
            return Some(race);
        }
        self.look_def(r).and_then(|d| d.race)
    }

    /// Whether the actor is female now: a script's change or the creator's choice, else
    /// the record's.
    pub fn actor_female(&self, r: FormId) -> bool {
        if r == FormId::PLAYER_REF {
            if let Some(f) = self.player.look.as_ref().map(|l| l.female).or(self.player.female) {
                return f;
            }
        } else if let Some(f) = self.extra.get(&r).and_then(|e| e.female) {
            return f;
        }
        self.look_def(r).is_some_and(|a| a.female)
    }

    /// The record the actor's race and sex come from: its base's look source when
    /// BaseCopyFaceGenFrom gave one, else the Traits template's.
    fn look_def(&self, r: FormId) -> Option<&crate::data::ActorDef> {
        let base = crate::functions::base_of(self, r).unwrap_or(r);
        match self.face_from_base.get(&base) {
            Some(src) => self.data.actors.get(src),
            None => self.traits_def(r),
        }
    }

    /// The race `steps` age steps from `race` (positive older), stopping at the end of
    /// the chain.
    pub fn aged_race(&self, race: FormId, steps: i32) -> FormId {
        let links = if steps > 0 { &self.data.race_older } else { &self.data.race_younger };
        let mut at = race;
        for _ in 0..steps.unsigned_abs().min(MAX_STEPS as u32) {
            match links.get(&at) {
                Some(&next) => at = next,
                None => break,
            }
        }
        at
    }

    /// `actor.AgeRace steps` (module notes). Returns the race now.
    pub fn age_race(&mut self, r: FormId, steps: i32) -> Option<FormId> {
        let from = self.actor_race(r)?;
        let to = self.aged_race(from, steps);
        if to != from {
            self.set_race(r, to);
            self.appearance_changes.push(AppearanceChange { actor: r, reset: false });
        }
        Some(to)
    }

    /// `actor.MatchRace source` (`functions::fo3`): the actor takes `race`, drawn again
    /// as AgeRace's change is.
    pub(crate) fn change_race(&mut self, r: FormId, race: FormId) {
        if self.actor_race(r) != Some(race) {
            self.set_race(r, race);
            self.appearance_changes.push(AppearanceChange { actor: r, reset: false });
        }
    }

    fn set_race(&mut self, r: FormId, race: FormId) {
        if r == FormId::PLAYER_REF {
            match self.player.look.as_mut() {
                Some(l) => l.race = race,
                None => self.player.race = Some(race),
            }
        } else {
            self.extra.entry(r).or_default().race = Some(race);
        }
    }

    /// `actor.SexChange [female] [reset]` (module notes): `None` gives the other sex.
    pub fn sex_change(&mut self, r: FormId, female: Option<bool>, reset: bool) {
        let was = self.actor_female(r);
        let female = female.unwrap_or(!was);
        if r == FormId::PLAYER_REF {
            if let Some(l) = self.player.look.as_mut() {
                l.female = female;
            }
            self.player.female = Some(female);
        } else {
            self.extra.entry(r).or_default().female = Some(female);
        }
        if female != was {
            let reset = reset && r == FormId::PLAYER_REF;
            self.appearance_changes.push(AppearanceChange { actor: r, reset });
        }
    }

    /// `actor.MatchFaceGeometry source percent` (module notes).
    pub fn match_face_geometry(&mut self, actor: FormId, source: FormId, percent: f32) -> Result<(), String> {
        let percent = if percent.is_finite() { percent.clamp(0.0, 100.0) } else { 0.0 };
        let known = match self.player.look.as_ref() {
            Some(l) if source == FormId::PLAYER_REF => Some((l.fggs.clone(), l.fgga.clone())),
            _ => None,
        };
        if actor == FormId::PLAYER_REF {
            let ((fggs, fgga), Some(look)) = (known.ok_or("the player's face can only be matched to a face the game holds")?, self.player.look.as_mut()) else {
                return Err("the player has no face from the character creator yet".into());
            };
            let blend = |cur: &[f32], to: &[f32]| {
                let wide = |v: &[f32]| v.iter().map(|&x| f64::from(x)).collect::<Vec<_>>();
                blend_coefficients(&wide(cur), &wide(to), f64::from(percent)).into_iter().map(|x| x as f32).collect()
            };
            (look.fggs, look.fgga) = (blend(&look.fggs, &fggs), blend(&look.fgga, &fgga));
        } else {
            let (fggs, fgga) = known.unwrap_or_default();
            self.extra.entry(actor).or_default().face_matches.push(FaceMatch { source, percent, fggs, fgga });
        }
        self.appearance_changes.push(AppearanceChange { actor, reset: false });
        Ok(())
    }

    /// The race and sex changes since the last call (the engine draws the actors again).
    pub fn take_appearance_changes(&mut self) -> Vec<AppearanceChange> {
        std::mem::take(&mut self.appearance_changes)
    }
}
