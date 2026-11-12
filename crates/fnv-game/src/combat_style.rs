//! Combat styles and confidence: the numbers the combat AI decides with
//! (docs/design/combat-ai.md §10.4–§10.5).
//!
//! - **Combat style** [documented: GECK wiki Combat Style; layout xEdit CSTY]: an actor's
//!   CSTY is the one its current package names (PACK CNAM, "the Combat Style the actor
//!   will use when the package is being run" [documented: GECK wiki Category:Packages];
//!   that it wins over SetCombatStyle's [hypothesis], docs/spec-notes/packages.md §7.3),
//!   else the one SetCombatStyle gave the reference, else its record's ZNAM, from the
//!   template that supplies its traits, as the death item is [documented: GECK wiki
//!   Traits Tab - NPC and Traits Tab - Creatures list "Combat Style" among the tab's
//!   fields, AI Data Tab has none; Template Data, "Use Traits: Use the traits of the
//!   ActorBase"; #2138], else `DefaultCombatstyle`
//!   [hypothesis: the masters' style of that name, FormID 0x3D, is the default].
//! - **Melee decision** [documented: Combat Style, Advanced]: the chance to attack is the
//!   base Attack % Chance, plus the Hand-To-Hand bonus when the actor fights hand to hand,
//!   plus the attack skill modifier (`base + mult × skill / 100`), times the multiplier
//!   for being under attack or not; compared with a roll of 1–100. Otherwise the actor
//!   holds for a random time within the Hold Timer. The recoil and unconscious bonuses
//!   add while the opponent recoils or lies unconscious.
//! - **Power attacks** [documented: Combat Style, Power Attacks]: having decided to
//!   attack, an actor power-attacks by Power Attack % Chance plus the recoil/stagger and
//!   unconscious bonuses to it ([`power_attack_chance`]); which one by the five shares
//!   ([`power_dir_by_chance`]) when the style says "Choose Attack using % Chance", else
//!   by how it moves (the engine's; docs/spec-notes/combat-rules.md §10.1). The Power
//!   Attack Fatigue Modifier adds to the chance ([`power_attack_fatigue`]).
//! - **Maneuver decision, rushes, radius, targeting FOV, explosives**
//!   (docs/design/combat-ai.md §17): [`maneuver`] dodges or idles by the dodge chance,
//!   multipliers and timers; [`rush_distance`] is where a melee fighter rolls for a rushing
//!   attack; [`beyond_combat_radius`], [`outside_targeting_fov`] and
//!   [`Game::spares_spectators`] are the Combat Radius, Max Targeting FOV and Ignore
//!   Damaging Spectators rules.
//! - **Confidence** [documented: GECK wiki Confidence]: the rating (0 Cowardly …
//!   4 Foolhardy) gives a threshold (fConfidenceCowardly 1000, Cautious 0.375, Average
//!   0.1875, Brave 0.0375, Foolhardy 0); the threat ratio is my combat strength (health
//!   counting armour, times damage per second) over the enemy's; below the threshold
//!   the actor flees. Each actor gets a random confidence modifier for each fight
//!   (fCombatConfidenceModifierMin −0.25 to Max 1.0, not for Foolhardy actors), a
//!   positive one making it more confident: the threshold times (1 − modifier)
//!   [hypothesis: how the modifier applies is not said]. The check is made every random
//!   0 to fCombatThreatRatioUpdateTime (7.5) seconds, never before the actor has taken
//!   damage. Fleeing itself is not built (a later workstream): the engine logs the
//!   decision.

use crate::data::CombatStyleDef;
use crate::functions::actor::actor_value;
use crate::game::Game;
use fnv_sim::FormId;

impl<'a> Game<'a> {
    /// The combat style `actor` fights with (module notes).
    pub fn combat_style(&self, actor: FormId) -> Option<&'a CombatStyleDef> {
        let data = self.data;
        let id = self.combat_style_id(actor).or_else(|| data.by_edid("DefaultCombatstyle"))?;
        data.combat_styles.get(&id)
    }

    /// The combat style `actor`'s package, SetCombatStyle or record gives it (module
    /// notes), without the DefaultCombatstyle fall back; none that is no combat style.
    pub fn combat_style_id(&self, actor: FormId) -> Option<FormId> {
        let data = self.data;
        // The current package's, else SetCombatStyle's, else the record's (module notes).
        let extra = self.extra.get(&actor);
        let package = extra.and_then(|e| e.current_package).and_then(|p| self.package_def(&p)).and_then(|d| d.combat_style).filter(|id| data.combat_styles.contains_key(id));
        let set = extra.and_then(|e| e.combat_style);
        let own = package.or(set).or_else(|| self.traits_def(actor).and_then(|d| d.combat_style));
        own.filter(|id| data.combat_styles.contains_key(id))
    }

    /// `actor`'s confidence threshold (module notes), from its Confidence rating and the
    /// game settings.
    pub fn confidence_threshold(&mut self, actor: FormId) -> f64 {
        let rating = actor_value(self, actor, crate::actor_values::CONFIDENCE).unwrap_or(2.0);
        confidence_threshold(rating, |name, d| crate::combat::setting(self, name, d))
    }

    /// The confidence modifier's range for one fight [documented: fCombatConfidenceModifierMin/Max].
    pub fn confidence_modifier_range(&self) -> (f64, f64) {
        (crate::combat::setting(self, "fCombatConfidenceModifierMin", -0.25), crate::combat::setting(self, "fCombatConfidenceModifierMax", 1.0))
    }

    /// `actor`'s health now and its full health.
    pub fn health_now_and_full(&mut self, actor: FormId) -> (f64, f64) {
        let full = crate::functions::actor::actor_value_base(self, actor, crate::actor_values::HEALTH);
        let now = actor_value(self, actor, crate::actor_values::HEALTH).unwrap_or(full);
        (now, full)
    }

    /// The longest time between two flee checks [documented: fCombatThreatRatioUpdateTime].
    pub fn threat_ratio_update_time(&self) -> f64 {
        crate::combat::setting(self, "fCombatThreatRatioUpdateTime", 7.5)
    }

    /// `actor`'s fatigue percent (docs/design/combat-ai.md §17.1): 1 − Fatigue / base
    /// Fatigue, 0 rested to 1 spent; 0 without a base.
    pub fn fatigue_share(&mut self, actor: FormId) -> f32 {
        let full = crate::functions::actor::actor_value_base(self, actor, crate::actor_values::FATIGUE);
        if full <= 0.0 {
            return 0.0;
        }
        let now = actor_value(self, actor, crate::actor_values::FATIGUE).unwrap_or(full);
        (1.0 - now / full).clamp(0.0, 1.0) as f32
    }

    /// Whether `actor`, fighting with `cs`, holds an explosive shot that would catch a
    /// bystander (§17.6): not with Ignore Damaging Spectators, nor when its Responsibility
    /// is below iAvoidHurtingNonTargetsResponsibility [documented; the setting is in no
    /// master: [`AVOID_HURTING_RESPONSIBILITY`], hypothesis].
    pub fn spares_spectators(&mut self, actor: FormId, cs: &CombatStyleDef) -> bool {
        if cs.ignore_damaging_spectators {
            return false;
        }
        let least = crate::combat::setting(self, "iAvoidHurtingNonTargetsResponsibility", AVOID_HURTING_RESPONSIBILITY);
        actor_value(self, actor, crate::actor_values::RESPONSIBILITY).unwrap_or(50.0) >= least
    }
}

/// iAvoidHurtingNonTargetsResponsibility where the load order has none (§17.6)
/// [hypothesis: no master carries it].
pub const AVOID_HURTING_RESPONSIBILITY: f64 = 30.0;

/// A number 1–100 from a roll in [0, 1), for "a random number from 1-100" [documented:
/// Combat Style].
fn one_to_hundred(roll: f32) -> f32 {
    ((roll * 100.0).floor() + 1.0).clamp(1.0, 100.0)
}

/// A random time within `(min, max)` from a roll in [0, 1).
fn within((lo, hi): (f32, f32), roll: f32) -> f32 {
    lo + (hi - lo).max(0.0) * roll
}

/// Which way a dodge goes, against the line to the opponent (§17.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DodgeDir {
    Left,
    Right,
    Forward,
    Back,
}

impl DodgeDir {
    pub fn name(self) -> &'static str {
        match self {
            DodgeDir::Left => "left",
            DodgeDir::Right => "right",
            DodgeDir::Forward => "forward",
            DodgeDir::Back => "back",
        }
    }
}

/// What the maneuver decision made (§17.1): stay idle, or dodge one way, for so many
/// seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Maneuver {
    Idle(f32),
    Dodge(DodgeDir, f32),
}

/// The maneuver decision's chance to dodge, 0 and up (§17.1): Dodge % + the fatigue
/// modifier (`fatigue`, 0–1) + the encumbered speed modifier (`walk_speed`, units/s), times
/// the under-attack multiplier or not [documented: GECK wiki Combat Style].
pub fn dodge_chance(cs: &CombatStyleDef, fatigue: f32, walk_speed: f32, under_attack: bool) -> f32 {
    let base = cs.dodge_chance + cs.dodge_fatigue_mod.0 + cs.dodge_fatigue_mod.1 * fatigue + cs.encumbered_speed_mod.0 + cs.encumbered_speed_mod.1 * walk_speed;
    (base * if under_attack { cs.dodge_mult.0 } else { cs.dodge_mult.1 }).max(0.0)
}

/// The maneuver decision (§17.1): a dodge by [`dodge_chance`] against a roll of 1–100,
/// back / forward / sideways by their multipliers (sideways weighs 1), sideways only when a
/// second roll is within Left/Right %, left or right at even odds; otherwise idle for the
/// Idle Timer. `attacking`: its own blow is on the way. `rolls` in [0, 1): dodge, way,
/// left/right %, left or right, time.
pub fn maneuver(cs: &CombatStyleDef, fatigue: f32, walk_speed: f32, under_attack: bool, attacking: bool, rolls: [f32; 5]) -> Maneuver {
    let idle = Maneuver::Idle(within(cs.idle_timer, rolls[4]));
    if one_to_hundred(rolls[0]) > dodge_chance(cs, fatigue, walk_speed, under_attack) {
        return idle;
    }
    let back = (if under_attack { cs.dodge_back_mult.0 } else { cs.dodge_back_mult.1 }).max(0.0);
    let forward = (if attacking { cs.dodge_forward_mult.0 } else { cs.dodge_forward_mult.1 }).max(0.0);
    let x = rolls[1] * (back + forward + 1.0);
    let (dir, timer) = if x < back {
        (DodgeDir::Back, cs.dodge_back_timer)
    } else if x < back + forward {
        (DodgeDir::Forward, cs.dodge_forward_timer)
    } else if one_to_hundred(rolls[2]) <= cs.left_right_chance {
        (if rolls[3] < 0.5 { DodgeDir::Left } else { DodgeDir::Right }, cs.dodge_lr_timer)
    } else {
        return idle;
    };
    match within(timer, rolls[4]) {
        t if t > 0.0 => Maneuver::Dodge(dir, t),
        _ => idle,
    }
}

/// How far from its target a melee fighter with `reach` rolls for a rushing attack
/// (§17.2): reach × (1 + Rushing Attack Distance Mult) [hypothesis]; `None` when the
/// style never rushes.
pub fn rush_distance(cs: &CombatStyleDef, reach: f32) -> Option<f32> {
    (cs.rushing_attack_chance > 0.0).then(|| reach * (1.0 + cs.rushing_attack_distance_mult.max(0.0)))
}

/// The Power Attack Fatigue Modifier: base + mult × `fatigue` (0–1) [documented: Combat
/// Style, Advanced; §17.3].
pub fn power_attack_fatigue(cs: &CombatStyleDef, fatigue: f32) -> f32 {
    cs.power_attack_fatigue_mod.0 + cs.power_attack_fatigue_mod.1 * fatigue
}

/// Whether a target `from_anchor` units from where the fighter's fight began or it was last
/// hit is beyond its Combat Radius (§17.4); 0 is no limit [hypothesis].
pub fn beyond_combat_radius(cs: &CombatStyleDef, from_anchor: f32) -> bool {
    cs.combat_radius > 0.0 && from_anchor > cs.combat_radius
}

/// Whether a target `off` radians off the fighter's heading is outside its Max Targeting
/// FOV (§17.5); 0 is no limit [hypothesis].
pub fn outside_targeting_fov(cs: &CombatStyleDef, off: f32) -> bool {
    cs.max_targeting_fov > 0.0 && off.to_degrees() > cs.max_targeting_fov * 0.5
}

/// The threshold of a Confidence rating (0 Cowardly … 4 Foolhardy) [documented: GECK
/// wiki Confidence, the settings and their defaults]; `setting(name, default)` reads a
/// game setting.
pub fn confidence_threshold(rating: f64, setting: impl Fn(&str, f64) -> f64) -> f64 {
    match rating.round() as i64 {
        i64::MIN..=0 => setting("fConfidenceCowardly", 1000.0),
        1 => setting("fConfidenceCautious", 0.375),
        2 => setting("fConfidenceAverage", 0.1875),
        3 => setting("fConfidenceBrave", 0.0375),
        _ => 0.0,
    }
}

/// An actor's health for the threat ratio: its health counting its armour,
/// `health / (1 − armour rating / 100)` [documented: GECK wiki Confidence, Health
/// Calculation].
pub fn effective_health(health: f64, armour_rating: f64) -> f64 {
    health / (1.0 - (armour_rating / 100.0).clamp(0.0, 0.95))
}

/// The threat ratio: my combat strength (health × damage per second) over the enemy's;
/// above 1 I am likely to win [documented: GECK wiki Confidence].
pub fn threat_ratio(my_health: f64, my_dps: f64, enemy_health: f64, enemy_dps: f64) -> f64 {
    let enemy = enemy_health * enemy_dps;
    if enemy <= 0.0 {
        return f64::INFINITY;
    }
    my_health * my_dps / enemy
}

/// Whether an actor flees: a threat ratio below its threshold, the threshold scaled by
/// this fight's confidence modifier (module notes). Foolhardy (threshold 0) never.
pub fn flees(ratio: f64, threshold: f64, modifier: f64) -> bool {
    threshold > 0.0 && ratio < threshold * (1.0 - modifier).max(0.0)
}

/// What the melee decision knows of the opponent (docs/design/combat-ai.md §10.4, §14.3).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Opponent {
    /// In a blow now ("under attack").
    pub attacking: bool,
    /// Recoiling (its blow was blocked) or staggering.
    pub recoiling: bool,
    /// Blocking now.
    pub blocking: bool,
    /// Lying unconscious (`Game::is_out_cold`).
    pub unconscious: bool,
}

/// The melee decision's chance to attack, 0–100 (module notes): `skill` the actor's
/// attack skill, `hand_to_hand` whether it fights without a weapon, `opp` what its
/// opponent is doing: the Recoil/Stagger bonus while it recoils, the under-attack
/// multipliers, Attack During Block Mult while it blocks [documented: GECK wiki Combat
/// Style].
pub fn melee_attack_chance(cs: &CombatStyleDef, skill: f32, hand_to_hand: bool, opp: Opponent) -> f32 {
    let base = cs.attack_chance
        + if hand_to_hand { cs.hand_to_hand_bonus } else { 0.0 }
        + if opp.recoiling { cs.recoil_stagger_bonus } else { 0.0 }
        + if opp.unconscious { cs.unconscious_bonus } else { 0.0 }
        + cs.attack_skill_mod.0
        + cs.attack_skill_mod.1 * skill.clamp(0.0, 100.0) / 100.0;
    base * if opp.attacking { cs.attack_under_attack_mult } else { cs.attack_not_under_attack_mult } * if opp.blocking { cs.attack_during_block_mult } else { 1.0 }
}

/// The chance, 0–100, that an attack decided by the melee decision is a power attack
/// (module notes): Power Attack % Chance, plus Recoil/Stagger Bonus to Power Attack while
/// the opponent recoils, plus Unconscious Bonus to Power Attack while it is unconscious,
/// plus the Power Attack Fatigue Modifier at `fatigue` (0–1) [documented: GECK wiki
/// Combat Style].
pub fn power_attack_chance(cs: &CombatStyleDef, opp: Opponent, fatigue: f32) -> f32 {
    (cs.power_attack_chance + if opp.recoiling { cs.recoil_stagger_power_bonus } else { 0.0 } + if opp.unconscious { cs.unconscious_power_bonus } else { 0.0 } + power_attack_fatigue(cs, fatigue)).clamp(0.0, 100.0)
}

/// Which power attack "Choose Attack using % Chance" picks, as an index into the shares
/// (0 normal, 1 forward, 2 back, 3 left, 4 right): a number of 1–100 from `roll` (0..1)
/// against the running sums of the first four shares, the first it is within, else
/// right [documented: GECK wiki Combat Style, the five steps in that order].
pub fn power_dir_by_chance(shares: [f32; 5], roll: f32) -> usize {
    let n = ((roll * 100.0).floor() + 1.0).clamp(1.0, 100.0);
    let mut sum = 0.0;
    for (i, share) in shares.iter().take(4).enumerate() {
        sum += share.max(0.0);
        if n <= sum {
            return i;
        }
    }
    4
}

/// The melee decision's chance to block, 0–100: (Block % + the block skill modifier,
/// `base + mult × 50 / 100`) times the under-attack or not multiplier [documented: GECK
/// wiki Combat Style, "Modifier = Base + Mult * ( 50 / 100 )"].
pub fn melee_block_chance(cs: &CombatStyleDef, under_attack: bool) -> f32 {
    let base = cs.block_chance + cs.block_skill_mod.0 + cs.block_skill_mod.1 * 0.5;
    (base * if under_attack { cs.block_under_attack_mult } else { cs.block_not_under_attack_mult }).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_by_rating() {
        let s = |_: &str, d: f64| d;
        assert_eq!(confidence_threshold(0.0, s), 1000.0);
        assert_eq!(confidence_threshold(1.0, s), 0.375);
        assert_eq!(confidence_threshold(2.0, s), 0.1875);
        assert_eq!(confidence_threshold(3.0, s), 0.0375);
        assert_eq!(confidence_threshold(4.0, s), 0.0);
    }

    #[test]
    fn threat_ratio_and_fleeing() {
        // Even match: 1. Half the health against the same damage: 0.5.
        assert_eq!(threat_ratio(100.0, 10.0, 100.0, 10.0), 1.0);
        assert_eq!(threat_ratio(50.0, 10.0, 100.0, 10.0), 0.5);
        // Armour 10 counts as about 11 % more health.
        assert!((effective_health(100.0, 10.0) - 111.111).abs() < 0.01);
        // A cautious actor (0.375) flees when outmatched more than about 3 to 1; a
        // foolhardy one never; a confident fight (modifier 1) never.
        assert!(flees(0.3, 0.375, 0.0) && !flees(0.4, 0.375, 0.0));
        assert!(!flees(0.0, 0.0, 0.0));
        assert!(!flees(0.1, 0.375, 1.0));
        assert!(flees(0.4, 0.375, -0.25));
    }

    #[test]
    fn the_melee_attack_chance() {
        // FalloutNV.esm's DefaultCombatstyle: 40 %, +5 hand to hand, skill × 20 / 100,
        // × 0.75 under attack (observed 2026-10-12).
        let cs = CombatStyleDef { attack_chance: 40.0, hand_to_hand_bonus: 5.0, attack_skill_mod: (0.0, 20.0), attack_under_attack_mult: 0.75, attack_not_under_attack_mult: 1.0, ..Default::default() };
        let calm = Opponent::default();
        let swinging = Opponent { attacking: true, ..calm };
        assert!((melee_attack_chance(&cs, 50.0, true, calm) - 55.0).abs() < 1e-4);
        assert!((melee_attack_chance(&cs, 50.0, false, swinging) - 37.5).abs() < 1e-4);
    }

    #[test]
    fn the_melee_block_chance_and_its_terms() {
        // FalloutNV.esm's DefaultCombatstyle [observed: fnvtool esm decode].
        let cs = CombatStyleDef {
            block_chance: 30.0,
            attack_chance: 40.0,
            recoil_stagger_bonus: 5.0,
            attack_skill_mod: (0.0, 20.0),
            attack_under_attack_mult: 0.75,
            attack_not_under_attack_mult: 1.0,
            block_skill_mod: (0.0, 20.0),
            block_under_attack_mult: 2.0,
            block_not_under_attack_mult: 1.0,
            attack_during_block_mult: 0.5,
            ..Default::default()
        };
        assert!((melee_block_chance(&cs, true) - 80.0).abs() < 1e-4);
        assert!((melee_block_chance(&cs, false) - 40.0).abs() < 1e-4);
        let calm = Opponent::default();
        // Recoiling: + the bonus; blocking: × Attack During Block Mult.
        assert!((melee_attack_chance(&cs, 50.0, false, Opponent { recoiling: true, ..calm }) - 55.0).abs() < 1e-4);
        assert!((melee_attack_chance(&cs, 50.0, false, Opponent { blocking: true, ..calm }) - 25.0).abs() < 1e-4);
    }

    #[test]
    fn the_power_attack_chance_and_its_bonuses() {
        // FalloutNV.esm's CSNVLegateLanius: 45 %, +15 recoiling, +25 unconscious
        // [observed: fnvtool esm decode, 2026-10-20].
        let cs = CombatStyleDef { power_attack_chance: 45.0, recoil_stagger_power_bonus: 15.0, unconscious_power_bonus: 25.0, unconscious_bonus: 5.0, attack_not_under_attack_mult: 1.0, ..Default::default() };
        let calm = Opponent::default();
        assert_eq!(power_attack_chance(&cs, calm, 0.0), 45.0);
        assert_eq!(power_attack_chance(&cs, Opponent { recoiling: true, ..calm }, 0.0), 60.0);
        assert_eq!(power_attack_chance(&cs, Opponent { unconscious: true, ..calm }, 0.0), 70.0);
        assert_eq!(power_attack_chance(&cs, Opponent { recoiling: true, unconscious: true, ..calm }, 0.0), 85.0);
        // Never past 100.
        let all = CombatStyleDef { power_attack_chance: 100.0, ..cs.clone() };
        assert_eq!(power_attack_chance(&all, Opponent { recoiling: true, ..calm }, 0.0), 100.0);
        // The attack chance's own unconscious bonus.
        assert_eq!(melee_attack_chance(&cs, 0.0, false, Opponent { unconscious: true, ..calm }), 5.0);
    }

    #[test]
    fn the_power_attack_picked_by_chance() {
        // 20 each way: 1–20 normal … 81–100 right.
        let even = [20.0; 5];
        assert_eq!(power_dir_by_chance(even, 0.0), 0);
        assert_eq!(power_dir_by_chance(even, 0.199), 0);
        assert_eq!(power_dir_by_chance(even, 0.2), 1);
        assert_eq!(power_dir_by_chance(even, 0.5), 2);
        assert_eq!(power_dir_by_chance(even, 0.79), 3);
        assert_eq!(power_dir_by_chance(even, 0.8), 4);
        assert_eq!(power_dir_by_chance(even, 0.9999), 4);
        // CSSupermutantMelee: 10/30/0/30/30; a 0 share is never picked.
        let mutant = [10.0, 30.0, 0.0, 30.0, 30.0];
        assert_eq!(power_dir_by_chance(mutant, 0.05), 0);
        assert_eq!(power_dir_by_chance(mutant, 0.395), 1);
        assert_eq!(power_dir_by_chance(mutant, 0.405), 3);
        assert_eq!(power_dir_by_chance(mutant, 0.71), 4);
        assert!((0..100).all(|i| power_dir_by_chance(mutant, i as f32 / 100.0) != 2));
        // Sums past 100: the later ones never come.
        assert!((0..100).all(|i| power_dir_by_chance([60.0, 60.0, 10.0, 10.0, 10.0], i as f32 / 100.0) <= 1));
    }

    /// FalloutNV.esm's DefaultCombatstyle's maneuver values [observed: fnvtool esm decode,
    /// 2026-10-21].
    fn default_style() -> CombatStyleDef {
        CombatStyleDef {
            dodge_chance: 75.0,
            left_right_chance: 50.0,
            dodge_lr_timer: (0.5, 1.5),
            dodge_forward_timer: (0.5, 1.0),
            dodge_back_timer: (0.25, 0.75),
            idle_timer: (0.5, 1.5),
            dodge_fatigue_mod: (0.0, -20.0),
            encumbered_speed_mod: (-110.0, 1.0),
            dodge_mult: (1.0, 0.75),
            dodge_back_mult: (1.0, 0.7),
            dodge_forward_mult: (1.0, 0.5),
            power_attack_chance: 25.0,
            power_attack_fatigue_mod: (5.0, -10.0),
            rushing_attack_chance: 25.0,
            rushing_attack_distance_mult: 1.0,
            combat_radius: 10000.0,
            ..Default::default()
        }
    }

    #[test]
    fn the_dodge_chance_and_its_modifiers() {
        let cs = default_style();
        // A rested person at 86.8 units/s: 75 − 23.2; not under attack × 0.75.
        assert!((dodge_chance(&cs, 0.0, 86.8, true) - 51.8).abs() < 1e-3);
        assert!((dodge_chance(&cs, 0.0, 86.8, false) - 38.85).abs() < 1e-3);
        // Spent and at 110: 75 − 20.
        assert!((dodge_chance(&cs, 1.0, 110.0, true) - 55.0).abs() < 1e-3);
        // Never below 0; a style without dodges never dodges.
        assert_eq!(dodge_chance(&cs, 1.0, 0.0, true), 0.0);
        let none = CombatStyleDef { dodge_chance: 0.0, encumbered_speed_mod: (0.0, 0.0), ..cs.clone() };
        assert!(matches!(maneuver(&none, 0.0, 86.8, true, false, [0.0; 5]), Maneuver::Idle(_)));
    }

    #[test]
    fn the_maneuver_decision() {
        let cs = default_style();
        let m = |rolls| maneuver(&cs, 0.0, 86.8, true, false, rolls);
        // 51 is within 51.8: a dodge; 53 is not: idle for 0.5 + 1.0 × 0.5 s.
        assert_eq!(m([0.52, 0.1, 0.0, 0.0, 0.5]), Maneuver::Idle(1.0));
        // Weights back 1, forward 0.5, sideways 1 (of 2.5).
        assert_eq!(m([0.5, 0.1, 0.0, 0.0, 0.5]), Maneuver::Dodge(DodgeDir::Back, 0.5));
        assert_eq!(m([0.5, 0.5, 0.0, 0.0, 0.0]), Maneuver::Dodge(DodgeDir::Forward, 0.5));
        // Sideways: 31 within Left/Right 50, then left or right; 61 is not: idle.
        assert_eq!(m([0.5, 0.9, 0.3, 0.2, 1.0]), Maneuver::Dodge(DodgeDir::Left, 1.5));
        assert_eq!(m([0.5, 0.9, 0.3, 0.7, 0.0]), Maneuver::Dodge(DodgeDir::Right, 0.5));
        assert_eq!(m([0.5, 0.9, 0.6, 0.2, 0.0]), Maneuver::Idle(0.5));
        // Not under attack, back weighs 0.7; its own blow on the way, forward weighs 1.
        assert_eq!(maneuver(&cs, 0.0, 120.0, false, true, [0.0, 0.6, 0.0, 0.0, 0.0]), Maneuver::Dodge(DodgeDir::Forward, 0.5));
        // Timers of 0: no dodge.
        let still = CombatStyleDef { dodge_back_timer: (0.0, 0.0), ..cs.clone() };
        assert!(matches!(maneuver(&still, 0.0, 86.8, true, false, [0.0, 0.0, 0.0, 0.0, 0.5]), Maneuver::Idle(_)));
    }

    #[test]
    fn rushes_fatigue_radius_and_fov() {
        let cs = default_style();
        // Reach 100, mult 1: from 200; mult 0: from reach; chance 0: never.
        assert_eq!(rush_distance(&cs, 100.0), Some(200.0));
        assert_eq!(rush_distance(&CombatStyleDef { rushing_attack_distance_mult: 0.0, ..cs.clone() }, 100.0), Some(100.0));
        assert_eq!(rush_distance(&CombatStyleDef { rushing_attack_chance: 0.0, ..cs.clone() }, 100.0), None);
        // Rested +5, spent −5.
        assert_eq!(power_attack_fatigue(&cs, 0.0), 5.0);
        assert_eq!(power_attack_fatigue(&cs, 1.0), -5.0);
        assert_eq!(power_attack_chance(&cs, Opponent::default(), 0.0), 30.0);
        // Combat radius 10000; 0 is no limit.
        assert!(!beyond_combat_radius(&cs, 9999.0) && beyond_combat_radius(&cs, 10001.0));
        assert!(!beyond_combat_radius(&CombatStyleDef { combat_radius: 0.0, ..cs.clone() }, 1e9));
        // CSLibertyPrime's 140°: 70° either side; 0 is no limit.
        let prime = CombatStyleDef { max_targeting_fov: 140.0, ..cs.clone() };
        assert!(!outside_targeting_fov(&prime, 69f32.to_radians()) && outside_targeting_fov(&prime, 71f32.to_radians()));
        assert!(!outside_targeting_fov(&cs, std::f32::consts::PI));
    }
}
