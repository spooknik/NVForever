//! Combat-tab dialogue: the lines (and effort sounds) actors say in a fight
//! (docs/spec-notes/combat-dialogue.md).
//!
//! - **Hit, Crippled, HealthHalf, HealthQuarter** ([`Game::hit_lines`], from
//!   [`Game::hit`], so every hit on the player, an NPC or a creature): a hit that cripples
//!   a limb says Crippled; one that takes health to half or a quarter says HealthHalf or
//!   HealthQuarter (once per crossing); otherwise, or when the actor has no such line, Hit
//!   when the damage is more than `fCombatSpeakHitThreshold` of its health and (an NPC
//!   or creature) a random number is below `fCombatSpeakHitChance`, or (the player)
//!   either [documented: Combat Tab names both settings; the "and" for NPCs is fitted:
//!   in the original's NPC battles about 1 % of 217 hits were followed by a line,
//!   observed 2026-10-17, docs/spec-notes/npc-behaviour-compare.md §2].
//! - **Death** ([`Game::death_line`], from [`Game::kill`]): always, cutting the actor's
//!   line; the line goes on although its speaker is dead (`Game::update_speech`). The
//!   nearest living member of its group within [`RESPONSE_RADIUS`] then says
//!   DeathResponse once the Death line is over [documented: Combat Tab, "If a valid actor
//!   is nearby"; design: who is valid and how near], and no sooner than
//!   [`DEATH_RESPONSE_SECONDS`] after the last DeathResponse anyone said: a death within
//!   that time is answered by nobody [design: owner's playtest 2026-11-11, #2156, "they
//!   say a death response after each death"]. A long generic death file plays
//!   one cry, and the line ends with it ([`Game::cut_line_after`]; voice.md §9.1).
//! - **Attack, PowerAttack, FireExplosive** ([`Game::attack_line`], from the engine when
//!   the player or an NPC begins an attack): at most one per actor per random time between
//!   `fCombatDialogueAttackMinElapsedTime` and `…MaxElapsedTime` [documented: Combat Tab];
//!   an NPC or creature also only when a random number is below `fCombatSpeakAttackChance`
//!   [hypothesis fitted to the original: about 3 lines for some 175 attacks begun in its
//!   NPC battles, observed 2026-10-17; the player keeps the timer alone, our design].
//!   It cuts the actor's own Hit line [design].
//! - **WeaponBroke, NoAmmo** ([`Game::combat_line`], from the engine).
//! - **Answers** ([`Game::queue_response`]): after an Attack, FireExplosive, Flee or
//!   AvoidThreat line the nearest living member of the speaker's group within
//!   [`RESPONSE_RADIUS`] says AttackResponse, FireExplosiveResponse, FleeResponse or
//!   AvoidThreatResponse once that line is over [documented: Combat Tab, "If a valid actor
//!   is nearby and an Attack line is said, the other actor will say their AttackResponse
//!   line in response", and the same for the others; design: who is valid and how near,
//!   as DeathResponse]. NormalToAlert is answered by NormalToAlertResponse and
//!   NormalToCombat by StartCombatResponse [hypothesis: the topics are not on the page;
//!   NormalToAlertResponse's lines answer a noise heard ("Yeah, I heard it too"),
//!   StartCombatResponse's are a fighter's word as a fight begins ("Ready as I'll ever
//!   be"), observed in FalloutNV.esm's lines].
//! - **OnFire, Poisoned** ([`Game::effect_started_lines`], from
//!   `Game::apply_hit_effect`): when a hit's effect resisted by Fire Resistance or Poison
//!   Resistance takes hold (not when it only lasts longer) [documented: Combat Tab, "Said
//!   when an actor gets lit on fire", "when an actor has been poisoned"; hypothesis: the
//!   resistance tells the two apart].
//! - **KnockedBack** (from `Game::knock`): knocked down but not unconscious (PushActorAway,
//!   an explosion's knockdown) [documented: Combat Tab].
//! - **WeaponIneffective, ArmorIneffective** ([`Game::armour_lines`], from the engine
//!   with a hit's damage before and after DT): the attacker when DT cut its hit down to
//!   its floor (the weapon cannot get through), the one hit when one hit took
//!   [`ARMOUR_FAILED_SHARE`] of its health or more while its DT stopped
//!   [`ARMOUR_FAILED_DT`] of the hit or less; each at most once per
//!   [`INEFFECTIVE_SECONDS`] per actor [documented: Combat Tab, "when an actor's weapon
//!   fails to damage an enemy sufficiently", "when an actor's armor fails to protect them
//!   sufficiently"; design: the measures, the page gives none]. Every line of the four
//!   topics above in the masters is a companion's (VNPCFollowers) [observed].
//!
//! A response with a sound of its own (creature grunts, robot buzzes) plays that sound
//! instead of the voice: `nvf-play`'s `voice.rs`.

use crate::game::Game;
use fnv_sim::FormId;

/// The actor value Fire Resistance [documented: GECK wiki Actor Value list, code 52;
/// `functions::actor` SCRIPT_AV_NAMES].
pub const FIRE_RESISTANCE: u16 = 52;
use std::collections::BTreeMap;

/// The actor of the group nearest to one that died says DeathResponse when within this
/// many game units of it [design: a room's width; the documentation says "nearby"].
pub const RESPONSE_RADIUS: f32 = 1536.0;

/// Seconds of play between two DeathResponse lines, whoever says them [design: owner's
/// playtest 2026-11-11, #2156; the documentation gives no wait].
pub const DEATH_RESPONSE_SECONDS: f64 = 20.0;

/// The topics answered by a nearby member of the speaker's group, and their answers
/// (module notes).
pub const ANSWERS: [(&str, &str); 6] = [
    ("Attack", "AttackResponse"),
    ("FireExplosive", "FireExplosiveResponse"),
    ("Flee", "FleeResponse"),
    ("AvoidThreat", "AvoidThreatResponse"),
    ("NormalToAlert", "NormalToAlertResponse"),
    ("NormalToCombat", "StartCombatResponse"),
];

/// ArmorIneffective: one hit took at least this share of the actor's health [design].
pub const ARMOUR_FAILED_SHARE: f64 = 0.25;
/// … while the actor's DT stopped at most this share of the hit's damage [design].
pub const ARMOUR_FAILED_DT: f64 = 0.10;
/// WeaponIneffective and ArmorIneffective are said at most once per this many seconds by
/// one actor [design].
pub const INEFFECTIVE_SECONDS: f64 = 60.0;

/// An answer due: when, who says it, about whom, the topic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Due {
    pub at: f64,
    pub who: FormId,
    pub about: FormId,
    pub topic: &'static str,
}

/// The settings of the Combat tab's timing, with FalloutNV.esm's values as defaults
/// [observed 2026-10-16, `fnvtool esm decode`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    /// `fCombatSpeakHitThreshold`: Hit is said when the damage is more than this share of
    /// the actor's health.
    pub hit_threshold: f64,
    /// `fCombatSpeakHitChance`: or when a random number in [0, 1) is below this.
    pub hit_chance: f64,
    /// `fCombatDialogueAttackMin/MaxElapsedTime`: seconds between two Attack or
    /// PowerAttack lines of one actor.
    pub attack_wait: (f64, f64),
    /// `fCombatSpeakAttackChance`: an NPC's chance, per attack begun, to say its line.
    pub attack_chance: f64,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing { hit_threshold: 0.01, hit_chance: 0.01, attack_wait: (5.0, 30.0), attack_chance: 0.01 }
    }
}

impl Timing {
    pub fn read(g: &Game<'_>) -> Timing {
        let d = Timing::default();
        let s = |name: &str, v: f64| crate::combat::setting(g, name, v);
        Timing {
            hit_threshold: s("fCombatSpeakHitThreshold", d.hit_threshold),
            hit_chance: s("fCombatSpeakHitChance", d.hit_chance),
            attack_wait: (s("fCombatDialogueAttackMinElapsedTime", d.attack_wait.0), s("fCombatDialogueAttackMaxElapsedTime", d.attack_wait.1)),
            attack_chance: s("fCombatSpeakAttackChance", d.attack_chance),
        }
    }
}

/// Combat lines' state. Not part of the saved state.
#[derive(Debug, Clone, Default)]
pub struct CombatLines {
    /// Per actor: `Game::time` before which it says no Attack or PowerAttack line.
    pub next_attack: BTreeMap<FormId, f64>,
    /// Per actor: the lowest health mark it has said a line for (2 half, 4 quarter).
    pub marks: BTreeMap<FormId, u8>,
    /// Answers due (DeathResponse and [`ANSWERS`]).
    pub responses: Vec<Due>,
    /// Per actor and topic (WeaponIneffective, ArmorIneffective): `Game::time` before
    /// which it is not said again.
    pub next_said: BTreeMap<(FormId, &'static str), f64>,
    /// Lines tried since the engine last took them: topic, speaker, the line said (`None`:
    /// the actor has none).
    pub said: Vec<(&'static str, FormId, Option<FormId>)>,
    pub timing: Option<Timing>,
    /// `Game::time` before which nobody says DeathResponse ([`DEATH_RESPONSE_SECONDS`]).
    pub next_death_response: f64,
    rng: u64,
    /// The hit under way says no lines ([`Game::hit_without_lines`]).
    quiet: bool,
}

impl CombatLines {
    /// A number in [0, 1) from the module's own sequence, so that the game's random
    /// numbers stay as they were.
    fn unit(&mut self) -> f64 {
        self.rng = self.rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.rng >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The health mark a hit crossed, from `before` to `after` of `max`: 4 for a quarter, 2
/// for half, 0 none; `said` is the lowest mark already said (a mark is said once until
/// health rises above it again).
pub fn mark_crossed(before: f64, after: f64, max: f64, said: u8) -> u8 {
    if max <= 0.0 || after <= 0.0 {
        return 0;
    }
    let at = |m: u8| max / f64::from(m);
    [4u8, 2].into_iter().find(|&m| after <= at(m) && before > at(m) && said < m).unwrap_or(0)
}

/// The lowest mark an actor at `health` of `max` is still below (to forget marks it rose
/// above).
pub fn mark_below(health: f64, max: f64) -> u8 {
    if max <= 0.0 {
        0
    } else if health <= max / 4.0 {
        4
    } else if health <= max / 2.0 {
        2
    } else {
        0
    }
}

/// Whether a hit of `dealt` damage on an actor of `max` health says Hit, with `roll` a
/// random number in [0, 1): an NPC or creature (`npc`) needs both the threshold and the
/// chance, the player either (module notes).
pub fn says_hit(dealt: f64, max: f64, roll: f64, t: &Timing, npc: bool) -> bool {
    let over = max > 0.0 && dealt / max > t.hit_threshold;
    let chance = roll < t.hit_chance;
    if npc {
        over && chance
    } else {
        over || chance
    }
}

impl Game<'_> {
    fn combat_timing(&mut self) -> Timing {
        if let Some(t) = self.combat_lines.timing {
            return t;
        }
        let t = Timing::read(self);
        self.combat_lines.timing = Some(t);
        t
    }

    /// `actor` says its line of the Combat topic `topic` about `about` (the conditions'
    /// target), when the load order has the topic, the actor is alive and not already
    /// speaking, and no conversation or menu holds the game (as Detection lines,
    /// `Game::detection_bark`). Returns the line said.
    pub fn combat_line(&mut self, actor: FormId, about: Option<FormId>, topic: &'static str) -> Option<FormId> {
        if self.menu.is_some() || self.dialogue.is_some() || self.is_talking(actor) || self.is_dead(actor) {
            return None;
        }
        self.say_combat(actor, about, topic)
    }

    /// Whether `actor` is saying a line of the topic with editor ID `topic`.
    fn saying_topic(&self, actor: FormId, topic: &str) -> bool {
        let t = self.data.by_edid(topic);
        t.is_some() && self.speech.iter().any(|s| s.speaker == actor && Some(s.topic) == t)
    }

    fn say_combat(&mut self, actor: FormId, about: Option<FormId>, topic: &'static str) -> Option<FormId> {
        let data = self.data;
        let t = data.by_edid(topic).filter(|t| data.topic_defs.contains_key(t))?;
        let line = self.say(actor, about, t, false);
        self.combat_lines.said.push((topic, actor, line));
        if line.is_some() {
            self.queue_response(actor, topic);
        }
        line
    }

    /// `speaker` said a line of `topic`: when the topic is one of [`ANSWERS`], the nearest
    /// living member of its group within [`RESPONSE_RADIUS`] says the answer once the line
    /// is over (module notes).
    pub(crate) fn queue_response(&mut self, speaker: FormId, topic: &str) {
        let Some(&(_, answer)) = ANSWERS.iter().find(|(t, _)| *t == topic) else { return };
        self.queue_answer(speaker, answer);
    }

    /// The nearest living member of `speaker`'s group within [`RESPONSE_RADIUS`] says
    /// `answer` about it when its line is over.
    fn queue_answer(&mut self, speaker: FormId, answer: &'static str) {
        let Some(at) = crate::awareness::position(self, speaker) else { return };
        let ids: Vec<FormId> = self.awareness.actors.keys().copied().filter(|&o| o != speaker && o != FormId::PLAYER_REF).collect();
        let near = ids
            .into_iter()
            .filter(|&o| !self.is_dead(o) && self.bark_mates(speaker, o))
            .filter_map(|o| {
                let p = crate::awareness::position(self, o)?;
                let d = (p[0] - at[0]).hypot(p[1] - at[1]).hypot(p[2] - at[2]);
                (d <= RESPONSE_RADIUS).then_some((d, o))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, o)) = near {
            let seconds = self.speech.iter().find(|s| s.speaker == speaker).map_or(0.0, |s| s.remaining);
            self.combat_lines.responses.push(Due { at: self.time + seconds, who: o, about: speaker, topic: answer });
        }
    }

    /// A hit's effect took hold on `target` (`Game::apply_hit_effect`): one resisted by
    /// Fire Resistance says OnFire, one by Poison Resistance Poisoned (module notes).
    pub(crate) fn effect_started_lines(&mut self, target: FormId, caster: Option<FormId>, applied: &[crate::aid::Applied]) {
        let data = self.data;
        let resisted = |code: u16| applied.iter().any(|a| !a.extended && data.magic_effects.get(&a.effect).is_some_and(|m| m.resistance == Some(code)));
        if resisted(FIRE_RESISTANCE) {
            self.combat_line(target, caster, "OnFire");
        } else if resisted(crate::actor_values::POISON_RESISTANCE) {
            self.combat_line(target, caster, "Poisoned");
        }
    }

    /// A hit by `attacker` on `target` of `raw` damage before armour, `dt` its DT and
    /// `dealt` the damage after armour, with `floor` the share of a hit DT always lets
    /// through (`fnv_combat::tuning::CombatTuning::dt_floor`): WeaponIneffective and
    /// ArmorIneffective (module notes). Called by the engine after the hit.
    pub fn armour_lines(&mut self, attacker: Option<FormId>, target: FormId, raw: f64, dt: f64, dealt: f64, floor: f64) {
        if raw <= 0.0 {
            return;
        }
        if let Some(a) = attacker.filter(|&a| a != target) {
            // DT took the hit down to its floor: what got through is the floor's share.
            if dt > 0.0 && raw - dt <= raw * floor {
                self.timed_line(a, Some(target), "WeaponIneffective");
            }
        }
        let max = self.max_health(target);
        if max > 0.0 && dealt >= ARMOUR_FAILED_SHARE * max && dt <= ARMOUR_FAILED_DT * raw && !self.is_dead(target) {
            self.timed_line(target, attacker, "ArmorIneffective");
        }
    }

    /// A combat line said at most once per [`INEFFECTIVE_SECONDS`] by one actor.
    fn timed_line(&mut self, actor: FormId, about: Option<FormId>, topic: &'static str) {
        let now = self.time;
        if self.combat_lines.next_said.get(&(actor, topic)).is_some_and(|&t| now < t) {
            return;
        }
        if self.combat_line(actor, about, topic).is_some() {
            self.combat_lines.next_said.insert((actor, topic), now + INEFFECTIVE_SECONDS);
        }
    }

    /// `actor` begins an attack on `target`: a power attack, the throw of a grenade or
    /// the shot of an explosive, or another. It says PowerAttack, FireExplosive or Attack
    /// once its attack timer has run out, which starts again on every try (said or not:
    /// an actor with no line does not look for one on every shot) [documented timer:
    /// Combat Tab; design: FireExplosive shares it].
    pub fn attack_line(&mut self, actor: FormId, target: Option<FormId>, power: bool, explosive: bool) -> Option<FormId> {
        let now = self.time;
        if self.combat_lines.next_attack.get(&actor).is_some_and(|&t| now < t) {
            return None;
        }
        // The attack's effort cuts the actor's own Hit grunt: a blow taken just before the
        // swing would silence it otherwise (combat-dialogue.md §2).
        let cuts_hit = self.saying_topic(actor, "Hit");
        if self.menu.is_some() || self.dialogue.is_some() || (self.is_talking(actor) && !cuts_hit) || self.is_dead(actor) {
            return None;
        }
        let t = self.combat_timing();
        if actor != FormId::PLAYER_REF && self.combat_lines.unit() >= t.attack_chance {
            return None;
        }
        let wait = t.attack_wait.0 + (t.attack_wait.1 - t.attack_wait.0).max(0.0) * self.combat_lines.unit();
        self.combat_lines.next_attack.insert(actor, now + wait);
        let topic = if power {
            "PowerAttack"
        } else if explosive {
            "FireExplosive"
        } else {
            "Attack"
        };
        if !cuts_hit {
            return self.say_combat(actor, target, topic).or_else(|| if topic == "Attack" { None } else { self.say_combat(actor, target, "Attack") });
        }
        // Cut the Hit line only for a line that takes its place.
        let data = self.data;
        let listener = target.or(Some(FormId::PLAYER_REF));
        let topics: &[&'static str] = if topic == "Attack" { &["Attack"] } else { &[topic, "Attack"] };
        for &name in topics {
            let Some(t) = data.by_edid(name).filter(|t| data.topic_defs.contains_key(t)) else { continue };
            let Some(info) = self.select_line(actor, listener, t) else {
                self.emit(crate::trace::EventKind::NoLine { speaker: actor, topic: t });
                self.combat_lines.said.push((name, actor, None));
                continue;
            };
            self.cut_line(actor);
            self.say_line(actor, target, t, info, crate::game::LineKind::Say);
            self.combat_lines.said.push((name, actor, Some(info)));
            self.queue_response(actor, name);
            return Some(info);
        }
        None
    }

    /// The lines of a hit that took `target` from `before` to `after` health of `max`
    /// (called by [`Game::hit`]; see the module notes).
    pub(crate) fn hit_lines(&mut self, target: FormId, attacker: Option<FormId>, before: f64, after: f64, max: f64, crippled: bool) {
        if self.is_dead(target) || self.combat_lines.quiet {
            return;
        }
        let below = mark_below(after, max);
        let said = self.combat_lines.marks.get(&target).copied().unwrap_or(0).min(below);
        let mark = mark_crossed(before, after, max, said);
        if below == 0 {
            self.combat_lines.marks.remove(&target);
        } else {
            self.combat_lines.marks.insert(target, said.max(mark));
        }
        if crippled && self.combat_line(target, attacker, "Crippled").is_some() {
            return;
        }
        let health_topic = match mark {
            4 => Some("HealthQuarter"),
            2 => Some("HealthHalf"),
            _ => None,
        };
        if health_topic.is_some_and(|t| self.combat_line(target, attacker, t).is_some()) {
            return;
        }
        // Friendly fire (a hit from an ally or friend) gets its Hit line only while the
        // victim's package allows "Friendly Fire Comments" [documented: GECK wiki
        // Category:Packages; docs/spec-notes/packages.md §7.6].
        let friendly = attacker.is_some_and(|a| a != target && matches!(self.reaction_to(target, a), crate::provocation::ALLY | crate::provocation::FRIEND));
        if friendly && !self.package_allows(target, crate::package::fallout::FRIENDLY_FIRE_COMMENTS) {
            return;
        }
        let t = self.combat_timing();
        let roll = self.combat_lines.unit();
        if says_hit(before - after, max, roll, &t, target != FormId::PLAYER_REF) {
            self.combat_line(target, attacker, "Hit");
        }
    }

    /// [`Game::hit`] without the target's lines (Hit, Crippled, HealthHalf, HealthQuarter,
    /// and Death if it dies): damage that is no blow, such as drowning's [design: owner
    /// 2026-10-25, "should not hear the player hit sounds" while drowning, and no death
    /// sound].
    pub fn hit_without_lines(&mut self, target: FormId, damage: f64, info: crate::combat::HitInfo) -> crate::combat::HitResult {
        self.combat_lines.quiet = true;
        let r = self.hit(target, damage, info);
        self.combat_lines.quiet = false;
        r
    }

    /// `actor` died (called by [`Game::kill`]): it says Death, cutting what it was
    /// saying, and the nearest living member of its group near it will say DeathResponse
    /// when that line is over.
    pub(crate) fn death_line(&mut self, actor: FormId, killer: Option<FormId>) {
        self.combat_lines.next_attack.remove(&actor);
        self.combat_lines.marks.remove(&actor);
        if self.loading || self.menu.is_some() || self.dialogue.is_some() {
            return;
        }
        self.cut_line(actor);
        // A death by drowning (`Game::hit_without_lines`) is silent [design: owner
        // 2026-10-25, "remove the death sound effect from drowning to death"].
        if self.combat_lines.quiet {
            return;
        }
        if self.say_combat(actor, killer, "Death").is_none() {
            return;
        }
        self.queue_answer(actor, "DeathResponse");
    }

    /// The engine plays only one cry of `speaker`'s line `info` (a long generic death
    /// file, `fnv_voice::cry`, issue #1539): the line ends `seconds` from now instead of
    /// with its file, and an answer due about it (DeathResponse) comes when it ends. A
    /// line already shorter, or another line, is left as it is.
    pub fn cut_line_after(&mut self, speaker: FormId, info: FormId, seconds: f64) {
        let Some(s) = self.speech.iter_mut().find(|s| s.speaker == speaker && s.info == info) else { return };
        if seconds >= s.remaining {
            return;
        }
        let less = s.remaining - seconds;
        s.remaining = seconds;
        for d in self.combat_lines.responses.iter_mut().filter(|d| d.about == speaker) {
            d.at = (d.at - less).max(self.time + seconds);
        }
    }

    /// The answers that are due, DeathResponse among them (called each frame from `Game::update_speech`).
    pub(crate) fn combat_lines_tick(&mut self) {
        let now = self.time;
        if self.combat_lines.responses.iter().all(|r| r.at > now) {
            return;
        }
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.combat_lines.responses).into_iter().partition(|r| r.at <= now);
        self.combat_lines.responses = later;
        for d in due {
            let death = d.topic == "DeathResponse";
            if !self.present(d.who) || (death && now < self.combat_lines.next_death_response) {
                continue;
            }
            if self.combat_line(d.who, Some(d.about), d.topic).is_some() && death {
                self.combat_lines.next_death_response = now + DEATH_RESPONSE_SECONDS;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_marks_are_said_once_per_crossing() {
        // 100 → 60: no mark; 60 → 45: half; 45 → 20: quarter; again below: nothing.
        assert_eq!(mark_crossed(100.0, 60.0, 100.0, 0), 0);
        assert_eq!(mark_crossed(60.0, 45.0, 100.0, 0), 2);
        assert_eq!(mark_crossed(45.0, 20.0, 100.0, 2), 4);
        assert_eq!(mark_crossed(20.0, 10.0, 100.0, 4), 0);
        // One hit past both marks says the quarter.
        assert_eq!(mark_crossed(90.0, 10.0, 100.0, 0), 4);
        // Death says no mark.
        assert_eq!(mark_crossed(30.0, 0.0, 100.0, 0), 0);
        // Healed above half, then hit below again: said again.
        assert_eq!(mark_below(70.0, 100.0), 0);
        assert_eq!(mark_crossed(70.0, 40.0, 100.0, 0), 2);
    }

    #[test]
    fn hit_is_said_above_the_threshold_or_by_chance() {
        let t = Timing::default();
        // The player: either.
        assert!(says_hit(2.0, 100.0, 0.5, &t, false));
        assert!(!says_hit(0.5, 100.0, 0.5, &t, false));
        assert!(says_hit(0.5, 100.0, 0.005, &t, false));
        // An NPC: both.
        assert!(!says_hit(2.0, 100.0, 0.5, &t, true));
        assert!(!says_hit(0.5, 100.0, 0.005, &t, true));
        assert!(says_hit(2.0, 100.0, 0.005, &t, true));
    }
}
