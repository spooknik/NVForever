//! Step C1 (docs/design/combat.md §4): the combat content of a load order, read with the
//! record layouts in `schemas/fnv` (WEAP and AMMO in `objects.json`; PROJ, EXPL, BPTD,
//! AMEF, ENCH, SPEL in `combat.json`). Field meanings and sources are in
//! docs/spec-notes/combat-data.md; this module keeps the records' own units and names.
//!
//! Nothing is dropped silently: a value that is missing, null where a form is expected,
//! or points at a record of the wrong type is counted in [`CombatForms::issues`].

use std::collections::{BTreeMap, HashMap};

use fnv_esm::{LoadOrder, Record, Schema};
use fnv_sim::extract::{self, all, first, global, has_flag, int, local_fid, num, text, Decoded, Issues};
use fnv_sim::settings::{unhex, Setting};
use fnv_sim::FormId;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

/// The set flag names of a decoded flags value, in bit order.
fn flag_names(v: Option<&Value>) -> Vec<String> {
    v.and_then(|f| f.get("set")).and_then(Value::as_array).map(|a| a.iter().filter_map(|n| n.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

/// An enum value as the schema names it; a value the schema does not name comes back
/// as its number, so it stays visible.
fn label(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// The clip stem of a WEAP DNAM power attack override (`AttackCustom1Power` …
/// `AttackCustom5Power`); `None` for DEFAULT, `0?` and anything else.
pub fn power_override_stem(name: Option<&str>) -> Option<String> {
    let n = name?.to_ascii_lowercase();
    let k = n.strip_prefix("attackcustom")?.strip_suffix("power")?;
    matches!(k, "1" | "2" | "3" | "4" | "5").then_some(n)
}

fn f(v: Option<&Value>, key: &str) -> f32 {
    v.and_then(|v| num(v, key)).unwrap_or(0.0) as f32
}

fn u(v: Option<&Value>, key: &str) -> u32 {
    v.and_then(|v| int(v, key)).unwrap_or(0).max(0) as u32
}

fn path(v: Option<&Value>) -> Option<String> {
    v.and_then(text).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Resolves the file-local FormIDs of one record.
struct Ctx<'a> {
    lo: &'a LoadOrder<'a>,
    pi: usize,
    id: FormId,
    issues: Issues,
}

impl Ctx<'_> {
    /// A FormID field (a subrecord value or a struct member). Null (0) is `None`.
    fn fid(&mut self, what: &str, v: Option<&Value>) -> Option<FormId> {
        let local = local_fid(v?)?;
        match global(self.lo, self.pi, local) {
            Ok(g) => g,
            Err(l) => {
                let id = self.id;
                self.issues.note("FormID outside master list", || format!("{id} {what} 0x{l:08X}"));
                None
            }
        }
    }

    fn field(&mut self, what: &str, v: Option<&Value>, key: &str) -> Option<FormId> {
        self.fid(what, v.and_then(|v| v.get(key)))
    }
}

/// A weapon mod slot (WMI1–3 with the matching DNAM effect and values).
#[derive(Debug, Clone, Serialize)]
pub struct WeaponMod {
    pub slot: u8,
    /// The IMOD item.
    pub item: Option<FormId>,
    /// DNAM "Effect - Mod N" (Increase Damage, Decrease Spread, …).
    pub effect: Option<String>,
    pub value_a: f32,
    pub value_b: f32,
}

/// WEAP: `DATA`, `DNAM`, `CRDT` and the forms the weapon names.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WeaponStats {
    pub edid: Option<String>,
    pub name: Option<String>,
    // DATA
    pub value: i32,
    /// Maximum condition.
    pub health: i32,
    pub weight: f32,
    /// Per shot (shared among the shot's projectiles), per hit for melee.
    pub base_damage: i32,
    pub clip_size: u32,
    // DNAM
    pub anim_type: Option<String>,
    pub anim_mult: f32,
    pub attack_mult: f32,
    /// Melee reach multiplier (× fCombatDistance × the actor's scale).
    pub reach: f32,
    /// DNAM Flags 1 and Flags 2, by name.
    pub flags: Vec<String>,
    pub automatic: bool,
    pub ammo_use: u32,
    /// The spread cone's median angle in degrees.
    pub min_spread: f32,
    /// Unused in New Vegas; kept for reports.
    pub spread: f32,
    pub sight_fov: f32,
    /// The weapon's own projectile (the ammo's, when it has one, replaces it).
    pub projectile: Option<FormId>,
    pub projectile_count: u32,
    pub min_range: f32,
    pub max_range: f32,
    pub on_hit: Option<String>,
    /// Shots per second of an automatic weapon.
    pub fire_rate: f32,
    /// DNAM Reload Time and Jam Time, seconds (0: unset) [documented: xEdit WEAP]
    /// (docs/spec-notes/item-fields.md §1).
    pub reload_time: f32,
    pub jam_time: f32,
    /// DNAM rumble: left and right motor strength (0 to 1) and duration (seconds), when
    /// any is set [documented: xEdit WEAP "Rumble - Left Motor Strength" …].
    pub rumble: Option<(f32, f32, f32)>,
    pub attack_shots_per_sec: f32,
    /// Set only when the "Override - Action Points" flag is.
    pub action_points: Option<f32>,
    /// Set only when the "Override - Damage to Weapon Mult" flag is.
    pub damage_to_weapon_mult: Option<f32>,
    pub aim_arc: f32,
    pub skill: Option<String>,
    /// The resistance that applies (DamageResist, FireResist, …; "None" for none).
    pub resist: Option<String>,
    pub limb_dmg_mult: f32,
    pub sight_usage: f32,
    pub semi_auto_fire_delay: [f32; 2],
    pub strength_req: u32,
    pub skill_req: u32,
    /// DNAM Base VATS To-Hit Chance: the chance to hit the weapon in another actor's
    /// hands, percent [documented: GECK wiki Weapons] (VATS mode, vats-mode.md §17.2). Not
    /// part of any report.
    #[serde(skip)]
    pub base_vats_to_hit: u32,
    pub regen_rate: f32,
    pub kill_impulse: f32,
    pub impulse_dist: f32,
    pub reload_anim: Option<String>,
    pub attack_anim: Option<String>,
    /// DNAM "Reload Animation (Mod)": the reload once a clip mod is on (weapon-mods.md
    /// §2). Not part of any report.
    #[serde(skip)]
    pub reload_anim_mod: Option<String>,
    /// DNAM "Power Attack Animation Override" as a clip stem (`AttackCustom3Power` →
    /// `attackcustom3power`), `None` for DEFAULT (255) and 0 [documented: xEdit
    /// wbDefinitionsFNV.pas WEAP DNAM; the clips `1hmattackcustom1power.kf` …
    /// `2hmattackcustom5power.kf` are in the base meshes, observed]. Not part of any report.
    #[serde(skip)]
    pub power_attack_override: Option<String>,
    /// `NNAM` "Embedded Weapon Node": the node of a creature's body an embedded weapon is
    /// attached to [documented: GECK wiki Weapons, Embedded Weapon "Node"]; its shots leave
    /// from there (nvforever `npc_combat`). Not part of any report.
    #[serde(skip)]
    pub embedded_node: Option<String>,
    pub mods: Vec<WeaponMod>,
    /// From the mods on ([`WeaponStats::modded`]): the projectile's speed multiplier
    /// ("Increase Projectile Speed", `None` for 1) and every fourth shot using no ammunition
    /// ("Regenerate Ammo (shots)") (docs/spec-notes/weapon-data-fields.md §5). Not part of
    /// any report.
    #[serde(skip)]
    pub projectile_speed_mult: Option<f32>,
    #[serde(skip)]
    pub free_fourth_shot: bool,
    /// DNAM ended before `resist_type`: the short form xEdit allows (fields from the 37th
    /// on absent); the later fields then read as zero.
    pub short_dnam: bool,
    // CRDT
    pub crit_damage: u32,
    pub crit_mult: f32,
    pub crit_on_death: bool,
    /// SPEL applied on a critical hit.
    pub crit_effect: Option<FormId>,
    // other subrecords
    /// NAM0: an AMMO, or a FLST of acceptable ammo.
    pub ammo: Option<FormId>,
    /// EITM: the object effect (ENCH) applied on a hit.
    pub enchantment: Option<FormId>,
    pub impact_dataset: Option<FormId>,
    pub embedded: bool,
    /// DNAM "Embedded Weapon - Actor Value" as a limb-condition actor value code (25
    /// Perception Condition … 31 Brain Condition): "the condition value affected when the
    /// weapon is targeted" [documented: GECK wiki Weapons; codes: [`limb_code`]]. `None`
    /// for a weapon that is not embedded. Not part of any report.
    #[serde(skip)]
    pub embedded_limb: Option<u16>,
    pub playable: bool,
    pub sound_level: Option<String>,
    /// MOD3: the scope's overlay model (docs/spec-notes/weapon-scopes.md §1). Not part of
    /// any report.
    #[serde(skip)]
    pub scope_nif: Option<String>,
    /// EFSD: the scope's effect shader, which the original ignores (weapon-scopes.md §1).
    #[serde(skip)]
    pub scope_effect: Option<FormId>,
    /// A mod with the Increase Zoom effect is on ([`WeaponStats::modded`]): the scope of a
    /// "Scope from Mod" weapon is there.
    #[serde(skip)]
    pub zoom_mod_on: bool,
}

/// AMMO.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AmmoStats {
    pub edid: Option<String>,
    pub name: Option<String>,
    pub short_name: Option<String>,
    pub abbrev: Option<String>,
    pub value: i32,
    pub clip_rounds: u32,
    pub flags: Vec<String>,
    /// Replaces the weapon's projectile when set.
    pub projectile: Option<FormId>,
    /// Replaces the weapon's count when non-zero.
    pub projectiles_per_shot: u32,
    pub weight: f32,
    /// Item added to the shooter's inventory with this percentage chance per shot.
    pub consumed: Option<(FormId, f32)>,
    /// AMEF effects (RCIL).
    pub effects: Vec<FormId>,
}

/// AMEF.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AmmoEffect {
    pub edid: Option<String>,
    pub name: Option<String>,
    /// Damage Mod, DR Mod, DT Mod, Spread Mod, Weapon Condition Mod, Fatigue Mod.
    pub kind: Option<String>,
    /// Add, Multiply, Subtract.
    pub operation: Option<String>,
    pub value: f32,
}

/// PROJ.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProjectileStats {
    pub edid: Option<String>,
    pub name: Option<String>,
    pub model: Option<String>,
    /// Missile, Lobber, Beam, Flame, Continuous Beam.
    pub kind: Option<String>,
    pub flags: Vec<String>,
    pub gravity: f32,
    pub speed: f32,
    pub range: f32,
    pub tracer_chance: f32,
    pub impact_force: f32,
    pub fade_duration: f32,
    pub muzzle_flash_light: Option<FormId>,
    pub muzzle_flash_duration: f32,
    pub muzzle_flash_model: Option<String>,
    /// The EXPL field; it fires only with the "Explosion" flag (see [`Self::explodes`]).
    pub explosion: Option<FormId>,
    pub alt_trigger_timer: f32,
    pub alt_trigger_proximity: f32,
    pub default_weapon: Option<FormId>,
    pub bouncy_mult: f32,
    pub sound_level: Option<String>,
}

/// The flag of a mod slot (1, 2, 3) in a unit's mod flags [documented: GECK wiki
/// GetEquippedWeaponModFlags: 1 Weapon Mod 1, 2 Weapon Mod 2, 4 Weapon Mod 3].
pub fn mod_flag(slot: u8) -> u8 {
    match slot {
        1..=3 => 1 << (slot - 1),
        _ => 0,
    }
}

impl WeaponStats {
    /// The slot whose mod item is `item`, if one is.
    pub fn mod_slot(&self, item: FormId) -> Option<&WeaponMod> {
        self.mods.iter().find(|m| m.item == Some(item) && mod_flag(m.slot) != 0)
    }

    /// The weapon with the mods `flags` names on: each slot's effect applied by its
    /// values as the GECK wiki's "Weapon Mod Effects" describes them
    /// (docs/spec-notes/weapon-mods.md §2, weapon-data-fields.md §5). Effects NVForever
    /// does not apply (equip time; VATS bonus, unused in the game) leave the figures as
    /// they are.
    pub fn modded(&self, flags: u8) -> WeaponStats {
        let mut w = self.clone();
        for m in self.mods.iter().filter(|m| flags & mod_flag(m.slot) != 0) {
            let (a, b) = (m.value_a, m.value_b);
            match m.effect.as_deref() {
                Some("Increase Weapon Damage") => w.base_damage += a.round() as i32,
                Some("Increase Clip Capacity") => {
                    w.clip_size = (w.clip_size as i64 + a.round() as i64).max(0) as u32;
                    // [hypothesis: the "(Mod)" reload is for the bigger magazine]
                    if w.reload_anim_mod.is_some() {
                        w.reload_anim = w.reload_anim_mod.clone();
                    }
                }
                Some("Decrease Spread") => w.min_spread = (w.min_spread - a).max(0.0),
                Some("Decrease Weight") => w.weight = (w.weight - a).max(0.0),
                Some("Increase Max. Condition") => w.health += a.round() as i32,
                // [hypothesis: "rate of fire" speeds both an automatic's fire rate and
                // the attack]
                Some("Increase Rate of Fire") => {
                    w.fire_rate *= 1.0 + a;
                    w.attack_shots_per_sec *= 1.0 + a;
                    w.attack_mult *= 1.0 + a;
                }
                Some("Increase Zoom") => {
                    w.sight_fov = (w.sight_fov - a).max(1.0);
                    w.zoom_mod_on = true;
                }
                Some("Split Beam") => {
                    w.projectile_count = (w.projectile_count.max(1) as i64 + a.round() as i64).max(1) as u32;
                    if b != 0.0 {
                        w.min_spread *= b;
                    }
                }
                // [hypothesis: the weapon's own noise]
                Some("Silence" | "Suppressor") => w.sound_level = Some("Silent".into()),
                // "Value #1 is the multiplier of the projectile speed" [documented].
                Some("Increase Projectile Speed") if a > 0.0 => w.projectile_speed_mult = Some(w.projectile_speed_mult.unwrap_or(1.0) * a),
                // "After 3 shots, the weapon's 4th shot then ignores the next use of
                // ammunition" [documented]; the values do nothing.
                Some("Regenerate Ammo (shots)") => w.free_fourth_shot = true,
                // Seconds added to the recharge time, on a weapon that recharges
                // [documented; hypothesis: only then], never below 0.1 s [design].
                Some("Regenerate Ammo (seconds)") if w.regen_rate > 0.0 => w.regen_rate = (w.regen_rate + a).max(0.1),
                _ => {}
            }
        }
        w
    }

    /// A Silence or Suppressor mod is among the mods `flags` names on: the weapon then
    /// shoots with its mod sounds (`WMS1`, `WMS2`) [documented: GECK wiki Weapons, mod
    /// effects] (docs/spec-notes/weapon-mods.md §7).
    pub fn sound_mod_on(&self, flags: u8) -> bool {
        self.mods.iter().any(|m| flags & mod_flag(m.slot) != 0 && matches!(m.effect.as_deref(), Some("Silence" | "Suppressor")))
    }

    /// Whether a DNAM flag (Flags 1 or Flags 2) is set, by name.
    pub fn has_flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }

    /// The scope's overlay model (MOD3) when the weapon is scoped: "Has Scope", and for a
    /// "Scope from Mod" weapon an Increase Zoom mod on it [documented: GECK wiki
    /// "Weapons", docs/spec-notes/weapon-scopes.md §2].
    pub fn scope_model(&self) -> Option<&str> {
        let there = self.has_flag("Has Scope") && (!self.has_flag("Scope from Mod") || self.zoom_mod_on);
        self.scope_nif.as_deref().filter(|m| there && !m.is_empty())
    }

    /// The scope gives night vision ("Scope has NightVision") while it is up.
    pub fn night_vision_scope(&self) -> bool {
        self.scope_model().is_some() && self.has_flag("Scope has NightVision")
    }

    /// Held in two hands: its DNAM animation type is one of the "(2 Hand)" types
    /// (rifles, automatic rifles, launchers, handles, two-handed melee) [observed: the
    /// schema's names].
    pub fn two_handed(&self) -> bool {
        self.anim_type.as_deref().is_some_and(|t| t.contains("(2 Hand)"))
    }
}

impl ProjectileStats {
    pub fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }

    /// Whether what this projectile was lands as its weapon, to be picked up again: DATA
    /// flag "Can Be Picked Up" ("can be picked up by the player character after it has
    /// been disabled, or regardless of it if Can be Disabled isn't enabled" [documented:
    /// GECK wiki Projectile]) without "Can Be Disabled" (mines and C-4, which
    /// `fnv_combat::mine` and the disarming keep) and without an explosion. The masters'
    /// throwing knife, hatchet, spear and tomahawk (and the trap baseball and Old World
    /// Blues' meat) [observed].
    pub fn lands_as_weapon(&self) -> bool {
        self.has("Can Be Picked Up") && !self.has("Can Be Disabled") && !self.has("Explosion")
    }

    /// The explosion this projectile sets off, if it sets one off.
    pub fn explodes(&self) -> Option<FormId> {
        self.explosion.filter(|_| self.has("Explosion"))
    }
}

/// EXPL.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExplosionStats {
    pub edid: Option<String>,
    pub name: Option<String>,
    pub model: Option<String>,
    pub force: f32,
    pub damage: f32,
    pub radius: f32,
    pub is_radius: f32,
    pub flags: Vec<String>,
    pub impact_dataset: Option<FormId>,
    pub radiation_level: f32,
    pub radiation_dissipation_time: f32,
    pub radiation_radius: f32,
    /// EITM: the object effect applied to what the blast catches.
    pub enchantment: Option<FormId>,
    pub placed_object: Option<FormId>,
    pub image_space_modifier: Option<FormId>,
    pub sound_level: Option<String>,
}

/// One part of a BPTD.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BodyPart {
    pub name: Option<String>,
    /// The bone the part hangs from.
    pub node: Option<String>,
    pub vats_target: Option<String>,
    pub ik_start_node: Option<String>,
    pub damage_mult: f32,
    pub flags: Vec<String>,
    pub part_type: Option<String>,
    /// Crippled at this percentage of the actor's total health.
    pub health_percent: u32,
    /// The limb-condition actor value the part lowers.
    pub actor_value: Option<String>,
    pub to_hit_chance: u32,
    pub tracking_max_angle: f32,
    pub severable_explosion: Option<FormId>,
    pub severable_debris: Option<FormId>,
    pub severable_impact_dataset: Option<FormId>,
    pub explodable_chance_percent: u32,
    pub explodable_explosion: Option<FormId>,
    pub explodable_debris: Option<FormId>,
    pub explodable_impact_dataset: Option<FormId>,
    pub limb_replacement_model: Option<String>,
    pub limb_replacement_scale: f32,
    pub gore_target_bone: Option<String>,
    /// Debris pieces and their scale when the part is severed, and when it explodes.
    pub severable_debris_count: u32,
    pub severable_debris_scale: f32,
    pub explodable_debris_count: u32,
    pub explodable_debris_scale: f32,
    /// Blood decals when severed and when exploded.
    pub severable_decal_count: u32,
    pub explodable_decal_count: u32,
    /// Gore effects positioning: translation and rotation (radians, x y z) from
    /// [`BodyPart::gore_target_bone`] [documented: GECK wiki Body Part].
    pub gore_translate: [f32; 3],
    pub gore_rotate: [f32; 3],
}

impl BodyPart {
    /// The part's location as `GetHitLocation` numbers it (the BodyLocation enum: −1 none,
    /// 0 torso, 1–2 head, 3–4 left arm, 5–6 right arm, 7–9 left leg, 10–12 right leg, 13
    /// brain, 14 weapon) [documented: xEdit wbBodyLocationEnum; GECK wiki GetHitLocation].
    pub fn location(&self) -> i32 {
        const NAMES: [&str; 15] = [
            "Torso", "Head 1", "Head 2", "Left Arm 1", "Left Arm 2", "Right Arm 1", "Right Arm 2", "Left Leg 1", "Left Leg 2", "Left Leg 3", "Right Leg 1",
            "Right Leg 2", "Right Leg 3", "Brain", "Weapon",
        ];
        self.part_type.as_deref().and_then(|t| NAMES.iter().position(|n| *n == t)).map_or(-1, |i| i as i32)
    }

    /// What kind of limb the part is, from its location.
    pub fn limb(&self) -> Limb {
        match self.location() {
            0 => Limb::Torso,
            1 | 2 | 13 => Limb::Head,
            3..=6 => Limb::Arm,
            7..=12 => Limb::Leg,
            _ => Limb::Other,
        }
    }

    /// The part can be severed (BPND flag) [documented: GECK wiki Body Part].
    pub fn severable(&self) -> bool {
        self.flags.iter().any(|f| f == "Severable")
    }

    /// The part can explode (BPND flag) [documented: GECK wiki Body Part].
    pub fn explodable(&self) -> bool {
        self.flags.iter().any(|f| f == "Explodable")
    }

    /// The damage input for a hit on this part.
    pub fn hit_part(&self) -> crate::damage::HitPart {
        crate::damage::HitPart { damage_mult: if self.damage_mult > 0.0 { self.damage_mult } else { 1.0 }, head: self.limb() == Limb::Head }
    }
}

/// A body part's kind (from its BodyLocation), for hit volume sizes and flinches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Limb {
    Head,
    Torso,
    Arm,
    Leg,
    Other,
}

/// One model of a DEBR: the chance it appears and its file [documented: GECK wiki Debris;
/// observed layout: a percentage byte, the path ended by NUL, a collision byte].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DebrisModel {
    pub percent: u8,
    pub model: String,
    pub collision: bool,
}

impl DebrisModel {
    pub fn from_bytes(b: &[u8]) -> Option<DebrisModel> {
        let (&percent, rest) = b.split_first()?;
        let end = rest.iter().position(|&c| c == 0)?;
        let model = fnv_util::text::decode(&rest[..end]).trim().to_string();
        let collision = rest.get(end + 1).is_some_and(|&c| c != 0);
        (!model.is_empty()).then_some(DebrisModel { percent, model, collision })
    }
}

/// BPTD.
#[derive(Debug, Clone, Serialize)]
pub struct BodyPartData {
    pub edid: Option<String>,
    pub model: Option<String>,
    pub parts: Vec<BodyPart>,
    pub ragdoll: Option<FormId>,
}

/// ARMO: what protects (combat step C5).
#[derive(Debug, Clone, Default, Serialize)]
pub struct ArmorStats {
    pub edid: Option<String>,
    pub name: Option<String>,
    /// DATA health: the maximum condition.
    pub health: i32,
    /// DNAM DT.
    pub dt: f32,
    /// DNAM DR as stored (xEdit shows it ÷ 100; New Vegas armour stores 0, actors.md §1.1).
    pub dr_raw: i32,
    /// BMDT biped slots (bits).
    pub slots: u32,
}

impl ArmorStats {
    /// The Upper Body biped slot [documented: xEdit's biped flag names, as schemas/fnv
    /// decodes BMDT].
    pub const UPPER_BODY: u32 = 1 << 2;
    /// The slots on the head: Head, Hair, Headband, Hat, Eye Glasses, Mask [documented:
    /// the same names; observed: a power armour helmet sets exactly these six].
    pub const HEAD_SLOTS: u32 = 1 | 1 << 1 | 1 << 9 | 1 << 10 | 1 << 11 | 1 << 14;

    pub fn covers_head(&self) -> bool {
        self.slots & Self::HEAD_SLOTS != 0
    }

    pub fn covers_body(&self) -> bool {
        self.slots & Self::UPPER_BODY != 0
    }
}

/// One effect of an ENCH or SPEL (EFID + EFIT, then its conditions).
#[derive(Debug, Clone, Serialize)]
pub struct EffectItem {
    /// The MGEF base effect.
    pub base: Option<FormId>,
    pub magnitude: u32,
    pub area: u32,
    pub duration: u32,
    /// Self, Touch, Target.
    pub range: Option<String>,
    pub actor_value: Option<String>,
    pub conditions: u32,
}

/// ENCH (object effect) or SPEL (actor effect).
#[derive(Debug, Clone, Serialize)]
pub struct EffectForm {
    pub sig: String,
    pub edid: Option<String>,
    pub name: Option<String>,
    /// ENCH: Weapon or Apparel; SPEL: Actor Effect, Ability, Poison, ….
    pub kind: Option<String>,
    pub flags: Vec<String>,
    pub effects: Vec<EffectItem>,
}

/// The records a shot follows, resolved for one weapon and ammo.
#[derive(Debug, Clone, Serialize)]
pub struct ShotChain {
    pub weapon: FormId,
    /// The ammo the weapon accepts (NAM0 itself, or the AMMO entries of its FLST).
    pub ammo_choices: Vec<FormId>,
    /// The ammo used: the one asked for when the weapon accepts it, else the first choice.
    pub ammo: Option<FormId>,
    pub projectile: Option<FormId>,
    /// "ammo" or "weapon": whose projectile field gave `projectile`.
    pub projectile_from: Option<&'static str>,
    pub projectiles_per_shot: u32,
    /// The weapon's base damage shared evenly among the shot's projectiles.
    pub damage_per_projectile: f32,
    pub explosion: Option<FormId>,
    pub ammo_effects: Vec<FormId>,
}

#[derive(Default)]
pub struct CombatForms {
    pub weapons: BTreeMap<FormId, WeaponStats>,
    pub ammo: HashMap<FormId, AmmoStats>,
    pub ammo_effects: HashMap<FormId, AmmoEffect>,
    pub projectiles: HashMap<FormId, ProjectileStats>,
    pub explosions: HashMap<FormId, ExplosionStats>,
    pub body_parts: HashMap<FormId, BodyPartData>,
    /// ENCH and SPEL.
    pub effects: HashMap<FormId, EffectForm>,
    /// Every FLST (weapons name their ammo through them).
    pub form_lists: HashMap<FormId, Vec<FormId>>,
    pub armor: HashMap<FormId, ArmorStats>,
    /// DEBR: gore and other debris models (docs/spec-notes/dismemberment.md §2.3).
    pub debris: HashMap<FormId, Vec<DebrisModel>>,
    /// CREA `PNAM`: a creature's body part data (step C8) [documented: xEdit CREA].
    pub creature_body_parts: HashMap<FormId, FormId>,
    /// Game settings by lower-case Editor ID.
    pub settings: BTreeMap<String, Setting>,
    /// Signature of the winning record of every loaded form.
    pub sigs: HashMap<FormId, [u8; 4]>,
    pub edids: HashMap<String, FormId>,
    pub issues: Issues,
}

/// IPDS is only registered (its signature), so references to it can be checked.
const SIGS: &[&[u8; 4]] = &[b"WEAP", b"AMMO", b"AMEF", b"PROJ", b"EXPL", b"BPTD", b"ENCH", b"SPEL", b"FLST", b"GMST", b"IPDS", b"ARMO", b"CREA", b"DEBR"];

enum Loaded {
    Weapon(Box<WeaponStats>),
    Ammo(AmmoStats),
    AmmoEffect(AmmoEffect),
    Projectile(Box<ProjectileStats>),
    Explosion(Box<ExplosionStats>),
    BodyParts(BodyPartData),
    Effect(EffectForm),
    FormList(Vec<FormId>),
    Setting(Setting),
    Armor(ArmorStats),
    CreatureParts(Option<FormId>),
    Debris(Vec<DebrisModel>),
    Other,
}

impl CombatForms {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> CombatForms {
        let mut out = CombatForms::default();
        let mut wanted: Vec<(FormId, usize, &Record)> = Vec::new();
        for (&g, defs) in &lo.index {
            let Some(d) = defs.last() else { continue };
            let r = lo.records[d.plugin as usize][d.ordinal as usize];
            // Deleted records (flag 0x20) are not forms.
            if !SIGS.contains(&&r.header.sig) || r.header.flags & 0x20 != 0 {
                continue;
            }
            wanted.push((FormId(g), d.plugin as usize, r));
        }
        wanted.sort_by_key(|w| w.0);
        let loaded: Vec<_> = wanted
            .par_iter()
            .map(|&(id, pi, r)| {
                let mut cx = Ctx { lo, pi, id, issues: Issues::default() };
                // DEBR `DATA` puts a string mid-field, which the schema cannot express: it is
                // read from the raw bytes (combat-data.md §1.7).
                if &r.header.sig == b"DEBR" {
                    let subs = r.subrecords().unwrap_or_default();
                    let edid = subs.iter().find(|s| &s.sig == b"EDID").map(|s| String::from_utf8_lossy(s.data.split(|&b| b == 0).next().unwrap_or(&[])).into_owned());
                    let models = subs.iter().filter(|s| &s.sig == b"DATA").filter_map(|s| DebrisModel::from_bytes(&s.data)).collect();
                    return (id, r.header.sig, (edid, Loaded::Debris(models)), cx.issues);
                }
                let res = match extract::decode(r, schema) {
                    Ok(d) => (first(&d, b"EDID").and_then(text), load_one(&mut cx, &r.header.sig, &d)),
                    Err(e) => {
                        cx.issues.note("record does not decode", || format!("{id}: {e}"));
                        (None, Loaded::Other)
                    }
                };
                (id, r.header.sig, res, cx.issues)
            })
            .collect();
        for (id, sig, (edid, l), issues) in loaded {
            out.issues.merge(issues);
            out.sigs.insert(id, sig);
            if let Some(e) = &edid {
                out.edids.insert(e.to_ascii_lowercase(), id);
            }
            match l {
                Loaded::Weapon(w) => {
                    out.weapons.insert(id, *w);
                }
                Loaded::Ammo(a) => {
                    out.ammo.insert(id, a);
                }
                Loaded::AmmoEffect(a) => {
                    out.ammo_effects.insert(id, a);
                }
                Loaded::Projectile(p) => {
                    out.projectiles.insert(id, *p);
                }
                Loaded::Explosion(e) => {
                    out.explosions.insert(id, *e);
                }
                Loaded::BodyParts(b) => {
                    out.body_parts.insert(id, b);
                }
                Loaded::Effect(e) => {
                    out.effects.insert(id, e);
                }
                Loaded::FormList(l) => {
                    out.form_lists.insert(id, l);
                }
                Loaded::Armor(a) => {
                    out.armor.insert(id, a);
                }
                Loaded::CreatureParts(p) => {
                    if let Some(p) = p {
                        out.creature_body_parts.insert(id, p);
                    }
                }
                Loaded::Debris(m) => {
                    out.debris.insert(id, m);
                }
                Loaded::Setting(s) => {
                    if let Some(e) = edid {
                        out.settings.insert(e.to_ascii_lowercase(), s);
                    }
                }
                Loaded::Other => {}
            }
        }
        out
    }

    pub fn by_edid(&self, edid: &str) -> Option<FormId> {
        self.edids.get(&edid.to_ascii_lowercase()).copied()
    }

    /// A numeric game setting by Editor ID (any case).
    pub fn setting(&self, edid: &str) -> Option<f32> {
        self.settings.get(&edid.to_ascii_lowercase()).and_then(Setting::as_f32)
    }

    /// The body part data of an actor (step C8): a creature's own (`PNAM`) when `base` is
    /// a CREA that names one, else the BPTD whose model is the actor's skeleton
    /// (`DefaultBodyPartData` is `Characters\_Male\skeleton.NIF`) [observed: the BPTD
    /// models; hypothesis: NPCs use the BPTD of their skeleton], skipping the player's
    /// own (`PlayerBodyPartData`, FormID 0x1C) for others.
    pub fn body_parts_for(&self, base: FormId, skeleton: &str) -> Option<FormId> {
        if let Some(p) = self.creature_body_parts.get(&base) {
            return Some(*p);
        }
        let want = skeleton.replace('/', "\\").to_ascii_lowercase();
        let want = want.trim_start_matches("meshes\\");
        let mut found: Vec<FormId> = self
            .body_parts
            .iter()
            .filter(|(id, b)| id.0 != 0x1C && b.model.as_deref().is_some_and(|m| m.replace('/', "\\").to_ascii_lowercase().trim_start_matches("meshes\\") == want))
            .map(|(id, _)| *id)
            .collect();
        found.sort();
        found.first().copied()
    }

    /// The armour an actor wears out of the items it carries: the carried ARMO, each once,
    /// by DT (then stored DR) descending, then first appearance; each is worn unless it
    /// has no slot or a worn item shares one [observed rule: the owner's statement of
    /// 2026-10-08 as `fnv_actor::resolve` implements it, actors.md §2.3].
    pub fn worn_armor(&self, items: impl IntoIterator<Item = FormId>) -> Vec<FormId> {
        let mut cands: Vec<(usize, FormId, &ArmorStats)> = Vec::new();
        for (i, id) in items.into_iter().enumerate() {
            if let Some(a) = self.armor.get(&id) {
                if !cands.iter().any(|c| c.1 == id) {
                    cands.push((i, id, a));
                }
            }
        }
        cands.sort_by(|a, b| b.2.dt.total_cmp(&a.2.dt).then(b.2.dr_raw.cmp(&a.2.dr_raw)).then(a.0.cmp(&b.0)));
        let mut used = 0u32;
        let mut worn = Vec::new();
        for (_, id, a) in cands {
            if a.slots != 0 && a.slots & used == 0 {
                used |= a.slots;
                worn.push(id);
            }
        }
        worn
    }

    /// DT and DR (percent) of worn armour: the sums over the items (DR stored ÷ 100)
    /// [hypothesis: pieces add up].
    pub fn armor_values(&self, worn: &[FormId]) -> (f32, f32) {
        worn.iter().filter_map(|id| self.armor.get(id)).fold((0.0, 0.0), |(dt, dr), a| (dt + a.dt, dr + a.dr_raw as f32 / 100.0))
    }

    /// The AMMO forms a weapon accepts: NAM0 when it is an AMMO, the AMMO entries of
    /// NAM0 when it is a form list [documented: GECK wiki Weapons and Ammo, owner export
    /// 2026-10-09], in list order.
    pub fn ammo_choices(&self, weapon: &WeaponStats) -> Vec<FormId> {
        let Some(a) = weapon.ammo else { return Vec::new() };
        if self.ammo.contains_key(&a) {
            return vec![a];
        }
        self.form_lists.get(&a).map(|l| l.iter().copied().filter(|x| self.ammo.contains_key(x)).collect()).unwrap_or_default()
    }

    /// The ammo a weapon loads: the first of its choices the carrier has (`has`), else
    /// the first choice. The lists put the plain round first (AmmoList10mm: 10mm,
    /// 10mm Hollow Point, 10mm JHP) [observed]; owner's account of the game 2026-10-09:
    /// a weapon defaults to the plain round unless only another type is carried, going
    /// down the list [hypothesis until captured].
    pub fn ammo_for(&self, weapon: FormId, mut has: impl FnMut(FormId) -> bool) -> Option<FormId> {
        let choices = self.ammo_choices(self.weapons.get(&weapon)?);
        choices.iter().copied().find(|&a| has(a)).or_else(|| choices.first().copied())
    }

    /// The next ammo for Ammo Swap: the next choice down the list (wrapping) that the
    /// carrier has, or `current` when no other is carried [design].
    pub fn next_ammo(&self, weapon: FormId, current: FormId, mut has: impl FnMut(FormId) -> bool) -> Option<FormId> {
        let choices = self.ammo_choices(self.weapons.get(&weapon)?);
        let at = choices.iter().position(|&a| a == current).unwrap_or(0);
        (1..=choices.len()).map(|k| choices[(at + k) % choices.len()]).find(|&a| a != current && has(a)).or(Some(current))
    }

    /// Resolve the shot chain of `weapon` loaded with `ammo` (the first acceptable ammo
    /// when `ammo` is `None` or not accepted; [`Self::ammo_for`] picks by inventory).
    ///
    /// The ammo's projectile, when set, replaces the weapon's; so does a non-zero
    /// projectile count; the weapon's damage is shared among the projectiles
    /// [documented: GECK wiki Ammo]. A projectile sets off its explosion only with its
    /// "Explosion" flag [documented: GECK wiki Projectile].
    pub fn chain(&self, weapon: FormId, ammo: Option<FormId>) -> Option<ShotChain> {
        Some(self.chain_with(weapon, self.weapons.get(&weapon)?, ammo))
    }

    /// The same for `weapon` with the figures `w` (a modded weapon's,
    /// [`WeaponStats::modded`]).
    pub fn chain_with(&self, weapon: FormId, w: &WeaponStats, ammo: Option<FormId>) -> ShotChain {
        let ammo_choices = self.ammo_choices(w);
        let ammo = ammo.filter(|a| ammo_choices.contains(a)).or_else(|| ammo_choices.first().copied());
        let a = ammo.and_then(|a| self.ammo.get(&a));
        let (projectile, projectile_from) = match a.and_then(|a| a.projectile) {
            Some(p) => (Some(p), Some("ammo")),
            None => (w.projectile, w.projectile.map(|_| "weapon")),
        };
        let projectiles_per_shot = match a.map(|a| a.projectiles_per_shot) {
            Some(n) if n > 0 => n,
            _ => w.projectile_count,
        };
        let explosion = projectile.and_then(|p| self.projectiles.get(&p)).and_then(ProjectileStats::explodes);
        ShotChain {
            weapon,
            ammo_choices,
            ammo,
            projectile,
            projectile_from,
            projectiles_per_shot,
            damage_per_projectile: w.base_damage as f32 / projectiles_per_shot.max(1) as f32,
            explosion,
            ammo_effects: a.map(|a| a.effects.clone()).unwrap_or_default(),
        }
    }
}

fn load_one(cx: &mut Ctx, sig: &[u8; 4], d: &[Decoded]) -> Loaded {
    match sig {
        b"WEAP" => Loaded::Weapon(Box::new(weapon(cx, d))),
        b"AMMO" => Loaded::Ammo(ammo(cx, d)),
        b"AMEF" => {
            let v = first(d, b"DATA");
            Loaded::AmmoEffect(AmmoEffect {
                edid: first(d, b"EDID").and_then(text),
                name: first(d, b"FULL").and_then(text),
                kind: label(v.and_then(|v| v.get("type"))),
                operation: label(v.and_then(|v| v.get("operation"))),
                value: f(v, "value"),
            })
        }
        b"PROJ" => Loaded::Projectile(Box::new(projectile(cx, d))),
        b"EXPL" => Loaded::Explosion(Box::new(explosion(cx, d))),
        b"BPTD" => Loaded::BodyParts(body_parts(cx, d)),
        b"ENCH" | b"SPEL" => Loaded::Effect(effect_form(cx, sig, d)),
        b"FLST" => {
            let items = all(d, b"LNAM").cloned().collect::<Vec<_>>();
            Loaded::FormList(items.iter().filter_map(|v| cx.fid("LNAM", Some(v))).collect())
        }
        b"CREA" => Loaded::CreatureParts(cx.fid("PNAM", first(d, b"PNAM"))),
        b"ARMO" => {
            let dnam = first(d, b"DNAM");
            let slots = first(d, b"BMDT").and_then(|b| b.get("biped_flags")).and_then(|f| num(f, "value")).unwrap_or(0.0) as u32;
            Loaded::Armor(ArmorStats {
                edid: first(d, b"EDID").and_then(text),
                name: first(d, b"FULL").and_then(text),
                health: first(d, b"DATA").and_then(|v| int(v, "health")).unwrap_or(0) as i32,
                dt: f(dnam, "dt"),
                dr_raw: dnam.and_then(|v| int(v, "dr")).unwrap_or(0) as i32,
                slots,
            })
        }
        b"GMST" => {
            let edid = first(d, b"EDID").and_then(text).unwrap_or_default();
            match first(d, b"DATA").and_then(Value::as_str).and_then(unhex) {
                Some(bytes) => Loaded::Setting(Setting::from_bytes(&edid, &bytes)),
                None => Loaded::Other,
            }
        }
        _ => Loaded::Other,
    }
}

fn weapon(cx: &mut Ctx, d: &[Decoded]) -> WeaponStats {
    let data = first(d, b"DATA");
    let dn = first(d, b"DNAM");
    let crdt = first(d, b"CRDT");
    let flags1 = dn.and_then(|v| v.get("flags1"));
    let flags2 = dn.and_then(|v| v.get("flags2"));
    let f2 = |name: &str| flags2.is_some_and(|x| has_flag(x, name));
    let mut flags = flag_names(flags1);
    flags.extend(flag_names(flags2));
    let id = cx.id;
    if dn.is_none() {
        cx.issues.note("WEAP without DNAM", || id.to_string());
    }
    let mut mods = Vec::new();
    for (slot, sig) in [(1u8, b"WMI1"), (2, b"WMI2"), (3, b"WMI3")] {
        let item = cx.fid("WMI", first(d, sig));
        let effect = label(dn.and_then(|v| v.get(format!("effect_mod{slot}"))));
        let value_a = f(dn, &format!("value_a_mod{slot}"));
        let value_b = f(dn, &format!("value_b_mod{slot}"));
        if item.is_some() || value_a != 0.0 || value_b != 0.0 {
            mods.push(WeaponMod { slot, item, effect, value_a, value_b });
        }
    }
    WeaponStats {
        edid: first(d, b"EDID").and_then(text),
        name: first(d, b"FULL").and_then(text),
        value: data.and_then(|v| int(v, "value")).unwrap_or(0) as i32,
        health: data.and_then(|v| int(v, "health")).unwrap_or(0) as i32,
        weight: f(data, "weight"),
        base_damage: data.and_then(|v| int(v, "base_damage")).unwrap_or(0) as i32,
        clip_size: u(data, "clip_size"),
        anim_type: label(dn.and_then(|v| v.get("animation_type"))),
        anim_mult: f(dn, "animation_multiplier"),
        attack_mult: f(dn, "animation_attack_multiplier"),
        reach: f(dn, "reach"),
        automatic: flags1.is_some_and(|x| has_flag(x, "Is Automatic")),
        flags,
        ammo_use: u(dn, "ammo_use"),
        min_spread: f(dn, "min_spread"),
        spread: f(dn, "spread"),
        sight_fov: f(dn, "sight_fov"),
        projectile: cx.field("DNAM projectile", dn, "projectile"),
        projectile_count: u(dn, "projectile_count"),
        min_range: f(dn, "min_range"),
        max_range: f(dn, "max_range"),
        on_hit: label(dn.and_then(|v| v.get("on_hit"))),
        fire_rate: f(dn, "fire_rate"),
        reload_time: f(dn, "reload_time"),
        jam_time: f(dn, "jam_time"),
        rumble: Some((f(dn, "rumble_left_motor_strength"), f(dn, "rumble_right_motor_strength"), f(dn, "rumble_duration"))).filter(|r| r.0 > 0.0 || r.1 > 0.0 || r.2 > 0.0),
        attack_shots_per_sec: f(dn, "attack_shots_per_sec"),
        action_points: f2("Override - Action Points").then(|| f(dn, "override_action_points")),
        damage_to_weapon_mult: f2("Override - Damage to Weapon Mult").then(|| f(dn, "override_damage_to_weapon_mult")),
        aim_arc: f(dn, "aim_arc"),
        skill: label(dn.and_then(|v| v.get("skill"))),
        resist: label(dn.and_then(|v| v.get("resist_type"))),
        limb_dmg_mult: f(dn, "limb_dmg_mult"),
        sight_usage: f(dn, "sight_usage"),
        semi_auto_fire_delay: [f(dn, "semi_automatic_fire_delay_min"), f(dn, "semi_automatic_fire_delay_max")],
        strength_req: u(dn, "strength_req"),
        skill_req: u(dn, "skill_req"),
        base_vats_to_hit: u(dn, "base_vats_to_hit_chance"),
        regen_rate: f(dn, "regen_rate"),
        kill_impulse: f(dn, "kill_impulse"),
        impulse_dist: f(dn, "impulse_dist"),
        reload_anim: label(dn.and_then(|v| v.get("reload_animation"))),
        attack_anim: label(dn.and_then(|v| v.get("attack_animation"))),
        reload_anim_mod: label(dn.and_then(|v| v.get("reload_animation_mod"))),
        power_attack_override: power_override_stem(label(dn.and_then(|v| v.get("power_attack_animation_override"))).as_deref()),
        embedded_node: label(first(d, b"NNAM")).map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
        mods,
        projectile_speed_mult: None,
        free_fourth_shot: false,
        short_dnam: dn.is_some_and(|v| v.get("resist_type").is_none()),
        crit_damage: u(crdt, "critical_damage"),
        crit_mult: f(crdt, "crit_percent_mult"),
        crit_on_death: crdt.and_then(|v| v.get("flags")).is_some_and(|x| has_flag(x, "On Death")),
        crit_effect: cx.field("CRDT effect", crdt, "effect"),
        ammo: cx.fid("NAM0", first(d, b"NAM0")),
        enchantment: cx.fid("EITM", first(d, b"EITM")),
        impact_dataset: cx.fid("INAM", first(d, b"INAM")),
        embedded: flags1.is_some_and(|x| has_flag(x, "Embedded Weapon")),
        embedded_limb: flags1.is_some_and(|x| has_flag(x, "Embedded Weapon")).then(|| label(dn.and_then(|v| v.get("embedded_weapon_actor_value"))).and_then(|l| limb_code(&l))).flatten(),
        playable: !flags1.is_some_and(|x| has_flag(x, "Non-Playable")),
        sound_level: label(first(d, b"VNAM")),
        scope_nif: first(d, b"MOD3").and_then(text),
        scope_effect: cx.fid("EFSD", first(d, b"EFSD")),
        zoom_mod_on: false,
    }
}

/// The limb-condition actor value code of a DNAM "Embedded Weapon - Actor Value" name
/// (schema `objects.json`: Perception, Endurance, Left Attack, Right Attack, Left
/// Mobility, Right Mobility, Brain → 25 … 31: the limb conditions, Perception Condition
/// to Brain Condition, in the same order (`fnv_game::actor_values::LIMB_CONDITIONS`)).
pub fn limb_code(name: &str) -> Option<u16> {
    const NAMES: [&str; 7] = ["Perception", "Endurance", "Left Attack", "Right Attack", "Left Mobility", "Right Mobility", "Brain"];
    NAMES.iter().position(|n| *n == name).map(|i| 25 + i as u16)
}

fn ammo(cx: &mut Ctx, d: &[Decoded]) -> AmmoStats {
    let data = first(d, b"DATA");
    let dat2 = first(d, b"DAT2");
    let consumed = cx.field("DAT2 consumed_ammo", dat2, "consumed_ammo").map(|c| (c, f(dat2, "consumed_percentage")));
    let effects = all(d, b"RCIL").cloned().collect::<Vec<_>>();
    AmmoStats {
        edid: first(d, b"EDID").and_then(text),
        name: first(d, b"FULL").and_then(text),
        short_name: first(d, b"ONAM").and_then(text),
        abbrev: first(d, b"QNAM").and_then(text),
        value: data.and_then(|v| int(v, "value")).unwrap_or(0) as i32,
        clip_rounds: u(data, "clip_rounds"),
        flags: flag_names(data.and_then(|v| v.get("flags"))),
        projectile: cx.field("DAT2 projectile", dat2, "projectile"),
        projectiles_per_shot: u(dat2, "projectiles_per_shot"),
        weight: f(dat2, "weight"),
        consumed,
        effects: effects.iter().filter_map(|v| cx.fid("RCIL", Some(v))).collect(),
    }
}

fn projectile(cx: &mut Ctx, d: &[Decoded]) -> ProjectileStats {
    let v = first(d, b"DATA");
    ProjectileStats {
        edid: first(d, b"EDID").and_then(text),
        name: first(d, b"FULL").and_then(text),
        model: path(first(d, b"MODL")),
        kind: label(v.and_then(|v| v.get("type"))),
        flags: flag_names(v.and_then(|v| v.get("flags"))),
        gravity: f(v, "gravity"),
        speed: f(v, "speed"),
        range: f(v, "range"),
        tracer_chance: f(v, "tracer_chance"),
        impact_force: f(v, "impact_force"),
        fade_duration: f(v, "fade_duration"),
        muzzle_flash_light: cx.field("DATA muzzle_flash_light", v, "muzzle_flash_light"),
        muzzle_flash_duration: f(v, "muzzle_flash_duration"),
        muzzle_flash_model: path(first(d, b"NAM1")),
        explosion: cx.field("DATA explosion", v, "explosion"),
        alt_trigger_timer: f(v, "explosion_alt_trigger_timer"),
        alt_trigger_proximity: f(v, "explosion_alt_trigger_proximity"),
        default_weapon: cx.field("DATA default_weapon_source", v, "default_weapon_source"),
        bouncy_mult: f(v, "bouncy_mult"),
        sound_level: label(first(d, b"VNAM")),
    }
}

fn explosion(cx: &mut Ctx, d: &[Decoded]) -> ExplosionStats {
    let v = first(d, b"DATA");
    ExplosionStats {
        edid: first(d, b"EDID").and_then(text),
        name: first(d, b"FULL").and_then(text),
        model: path(first(d, b"MODL")),
        force: f(v, "force"),
        damage: f(v, "damage"),
        radius: f(v, "radius"),
        is_radius: f(v, "is_radius"),
        flags: flag_names(v.and_then(|v| v.get("flags"))),
        impact_dataset: cx.field("DATA impact_dataset", v, "impact_dataset"),
        radiation_level: f(v, "radiation_level"),
        radiation_dissipation_time: f(v, "radiation_dissipation_time"),
        radiation_radius: f(v, "radiation_radius"),
        enchantment: cx.fid("EITM", first(d, b"EITM")),
        placed_object: cx.fid("INAM", first(d, b"INAM")),
        image_space_modifier: cx.fid("MNAM", first(d, b"MNAM")),
        sound_level: label(v.and_then(|v| v.get("sound_level"))),
    }
}

/// BPTD parts: each starts at BPTN, or at BPNN when no BPTN opened it (the name is
/// optional) [documented: xEdit wbDefinitionsFNV.pas BPTD, dfStructFirstNotRequired].
fn body_parts(cx: &mut Ctx, d: &[Decoded]) -> BodyPartData {
    let mut parts: Vec<BodyPart> = Vec::new();
    let mut open = false;
    for s in d {
        let v = s.value.as_ref();
        match &s.sig {
            b"BPTN" => {
                parts.push(BodyPart { name: v.and_then(text), ..BodyPart::default() });
                open = true;
            }
            b"BPNN" => {
                if !open {
                    parts.push(BodyPart::default());
                }
                open = false;
                if let Some(p) = parts.last_mut() {
                    p.node = v.and_then(text);
                }
            }
            b"BPNT" | b"BPNI" | b"BPND" | b"NAM1" | b"NAM4" => {
                let Some(p) = parts.last_mut() else {
                    let id = cx.id;
                    cx.issues.note("BPTD part field before any part", || format!("{id} {}", String::from_utf8_lossy(&s.sig)));
                    continue;
                };
                match &s.sig {
                    b"BPNT" => p.vats_target = v.and_then(text),
                    b"BPNI" => p.ik_start_node = v.and_then(text),
                    b"NAM1" => p.limb_replacement_model = path(v),
                    b"NAM4" => p.gore_target_bone = v.and_then(text),
                    _ => {
                        p.damage_mult = f(v, "damage_mult");
                        p.flags = flag_names(v.and_then(|v| v.get("flags")));
                        p.part_type = label(v.and_then(|v| v.get("part_type")));
                        p.health_percent = u(v, "health_percent");
                        p.actor_value = label(v.and_then(|v| v.get("actor_value")));
                        p.to_hit_chance = u(v, "to_hit_chance");
                        p.tracking_max_angle = f(v, "tracking_max_angle");
                        p.explodable_chance_percent = u(v, "explodable_explosion_chance_percent");
                        p.limb_replacement_scale = f(v, "limb_replacement_scale");
                        p.severable_debris_count = v.and_then(|v| int(v, "severable_debris_count")).unwrap_or(0).max(0) as u32;
                        p.severable_debris_scale = f(v, "severable_debris_scale");
                        p.explodable_debris_count = u(v, "explodable_debris_count");
                        p.explodable_debris_scale = f(v, "explodable_debris_scale");
                        p.severable_decal_count = u(v, "severable_decal_count");
                        p.explodable_decal_count = u(v, "explodable_decal_count");
                        let gore = v.and_then(|v| v.get("gore_effects_positioning"));
                        let xyz = |k: &str| {
                            let o = gore.and_then(|g| g.get(k));
                            [f(o, "x"), f(o, "y"), f(o, "z")]
                        };
                        p.gore_translate = xyz("position");
                        p.gore_rotate = xyz("rotation_radians");
                    }
                }
                if &s.sig == b"BPND" {
                    let p_ix = parts.len() - 1;
                    let ids = [
                        cx.field("BPND severable_explosion", v, "severable_explosion"),
                        cx.field("BPND severable_debris", v, "severable_debris"),
                        cx.field("BPND severable_impact_dataset", v, "severable_impact_dataset"),
                        cx.field("BPND explodable_explosion", v, "explodable_explosion"),
                        cx.field("BPND explodable_debris", v, "explodable_debris"),
                        cx.field("BPND explodable_impact_dataset", v, "explodable_impact_dataset"),
                    ];
                    let p = &mut parts[p_ix];
                    [
                        p.severable_explosion,
                        p.severable_debris,
                        p.severable_impact_dataset,
                        p.explodable_explosion,
                        p.explodable_debris,
                        p.explodable_impact_dataset,
                    ] = ids;
                }
            }
            _ => {}
        }
    }
    BodyPartData {
        edid: first(d, b"EDID").and_then(text),
        model: path(first(d, b"MODL")),
        parts,
        ragdoll: cx.fid("RAGA", first(d, b"RAGA")),
    }
}

/// ENCH/SPEL: each effect starts at EFID; EFIT and CTDA that follow belong to it
/// [documented: xEdit wbEffects].
fn effect_form(cx: &mut Ctx, sig: &[u8; 4], d: &[Decoded]) -> EffectForm {
    let head = if sig == b"ENCH" { first(d, b"ENIT") } else { first(d, b"SPIT") };
    let mut effects: Vec<EffectItem> = Vec::new();
    for s in d {
        let v = s.value.as_ref();
        match &s.sig {
            b"EFID" => {
                let base = cx.fid("EFID", v);
                effects.push(EffectItem { base, magnitude: 0, area: 0, duration: 0, range: None, actor_value: None, conditions: 0 });
            }
            b"EFIT" => {
                if let Some(e) = effects.last_mut() {
                    e.magnitude = u(v, "magnitude");
                    e.area = u(v, "area");
                    e.duration = u(v, "duration");
                    e.range = label(v.and_then(|v| v.get("type")));
                    e.actor_value = label(v.and_then(|v| v.get("actor_value")));
                }
            }
            b"CTDA" => {
                if let Some(e) = effects.last_mut() {
                    e.conditions += 1;
                }
            }
            _ => {}
        }
    }
    EffectForm {
        sig: String::from_utf8_lossy(sig).into_owned(),
        edid: first(d, b"EDID").and_then(text),
        name: first(d, b"FULL").and_then(text),
        kind: label(head.and_then(|v| v.get("type"))),
        flags: flag_names(head.and_then(|v| v.get("flags"))),
        effects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thrown_weapons_land_and_mines_do_not() {
        let p = |flags: &[&str]| ProjectileStats { flags: flags.iter().map(|f| f.to_string()).collect(), ..Default::default() };
        // NVThrowingKnifeProjectile and the frag mine's flags (FalloutNV.esm).
        assert!(p(&["Alt. Trigger", "Can Be Picked Up", "Pins Limbs", "Rotation"]).lands_as_weapon());
        assert!(!p(&["Explosion", "Alt. Trigger", "Can Be Disabled", "Can Be Picked Up"]).lands_as_weapon());
        assert!(!p(&["Hitscan", "Supersonic"]).lands_as_weapon());
    }

    #[test]
    fn power_attack_override_stems() {
        assert_eq!(power_override_stem(Some("AttackCustom3Power")).as_deref(), Some("attackcustom3power"));
        assert_eq!(power_override_stem(Some("DEFAULT")), None);
        assert_eq!(power_override_stem(Some("0?")), None);
        assert_eq!(power_override_stem(Some("AttackCustom9Power")), None);
        assert_eq!(power_override_stem(None), None);
    }

    #[test]
    fn debris_data_bytes() {
        // GoreBits01's first DATA [observed: FalloutNV.esm 0x1B2BF].
        let b = b"\x5aGore\\MeatBit02.NIF\x00\x01";
        assert_eq!(DebrisModel::from_bytes(b), Some(DebrisModel { percent: 90, model: "Gore\\MeatBit02.NIF".into(), collision: true }));
        assert_eq!(DebrisModel::from_bytes(b"\x10"), None);
    }

    fn slot(slot: u8, item: u32, effect: &str, a: f32, b: f32) -> WeaponMod {
        WeaponMod { slot, item: Some(FormId(item)), effect: Some(effect.into()), value_a: a, value_b: b }
    }

    /// The 10mm Pistol's slots as FalloutNV.esm has them [observed: weapon-mods.md §2].
    fn pistol() -> WeaponStats {
        WeaponStats {
            base_damage: 22,
            clip_size: 12,
            min_spread: 0.8,
            reload_anim: Some("ReloadA".into()),
            reload_anim_mod: Some("ReloadB".into()),
            sound_level: Some("Normal".into()),
            mods: vec![slot(1, 0x10C858, "Increase Clip Capacity", 4.0, 0.0), slot(2, 0x10C859, "Decrease Spread", 0.4, 0.0), slot(3, 0x129878, "Silence", 0.0, 0.0)],
            ..Default::default()
        }
    }

    #[test]
    fn the_flags_name_the_slots() {
        assert_eq!([mod_flag(1), mod_flag(2), mod_flag(3), mod_flag(0), mod_flag(4)], [1, 2, 4, 0, 0]);
        let w = pistol();
        assert_eq!(w.mod_slot(FormId(0x10C859)).map(|m| m.slot), Some(2));
        assert!(w.mod_slot(FormId(0x1)).is_none());
    }

    #[test]
    fn mods_change_the_figures_they_name() {
        let w = pistol();
        assert_eq!(w.modded(0).clip_size, 12);
        let clip = w.modded(1);
        assert_eq!((clip.clip_size, clip.reload_anim.as_deref()), (16, Some("ReloadB")));
        assert!((clip.min_spread - 0.8).abs() < 1e-6);
        let all = w.modded(7);
        assert_eq!(all.clip_size, 16);
        assert!((all.min_spread - 0.4).abs() < 1e-6);
        assert_eq!(all.sound_level.as_deref(), Some("Silent"));
        // Only the slots named: the silencer alone.
        assert!(w.sound_mod_on(4) && w.sound_mod_on(7));
        assert!(!w.sound_mod_on(0) && !w.sound_mod_on(3));
        let quiet = w.modded(4);
        assert_eq!((quiet.clip_size, quiet.sound_level.as_deref()), (12, Some("Silent")));
    }

    #[test]
    fn a_scope_from_a_mod_needs_an_increase_zoom_mod() {
        let flags = |f: &[&str]| f.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let sniper = WeaponStats { flags: flags(&["Has Scope"]), scope_nif: Some("mildotscope.nif".into()), ..Default::default() };
        assert_eq!(sniper.scope_model(), Some("mildotscope.nif"));
        assert!(!sniper.night_vision_scope());
        let hunting = WeaponStats {
            flags: flags(&["Has Scope", "Scope from Mod"]),
            scope_nif: Some("germanscope.nif".into()),
            mods: vec![slot(1, 1, "Increase Zoom", 20.0, 0.0), slot(2, 2, "Increase Clip Capacity", 5.0, 0.0)],
            sight_fov: 55.0,
            ..Default::default()
        };
        assert_eq!(hunting.scope_model(), None);
        assert_eq!(hunting.modded(2).scope_model(), None);
        assert_eq!(hunting.modded(1).scope_model(), Some("germanscope.nif"));
        let varmint = WeaponStats { flags: flags(&["Has Scope", "Scope has NightVision", "Scope from Mod"]), ..hunting.clone() };
        assert!(!varmint.night_vision_scope() && varmint.modded(1).night_vision_scope());
        // No flag, no scope, whatever MOD3 says.
        assert_eq!(WeaponStats { scope_nif: Some("x.nif".into()), ..Default::default() }.scope_model(), None);
    }

    #[test]
    fn each_effect_by_the_wiki() {
        let base = WeaponStats { base_damage: 10, weight: 5.0, health: 100, fire_rate: 10.0, attack_mult: 1.0, sight_fov: 55.0, projectile_count: 1, min_spread: 2.0, ..Default::default() };
        let with = |e: &str, a: f32, b: f32| WeaponStats { mods: vec![slot(1, 1, e, a, b)], ..base.clone() }.modded(1);
        assert_eq!(with("Increase Weapon Damage", 5.0, 0.0).base_damage, 15);
        assert!((with("Decrease Weight", 2.0, 0.0).weight - 3.0).abs() < 1e-6);
        assert!((with("Decrease Weight", 9.0, 0.0).weight).abs() < 1e-6);
        assert_eq!(with("Increase Max. Condition", 50.0, 0.0).health, 150);
        assert!((with("Increase Rate of Fire", 0.2, 0.0).fire_rate - 12.0).abs() < 1e-4);
        assert!((with("Increase Zoom", 15.0, 0.0).sight_fov - 40.0).abs() < 1e-6);
        assert!(with("Increase Zoom", 15.0, 0.0).zoom_mod_on && !with("Decrease Weight", 2.0, 0.0).zoom_mod_on);
        let split = with("Split Beam", 2.0, 1.5);
        assert_eq!(split.projectile_count, 3);
        assert!((split.min_spread - 3.0).abs() < 1e-6);
        // Value B 0 keeps the spread; fewer beams never go below one.
        assert!((with("Split Beam", -4.0, 0.0).min_spread - 2.0).abs() < 1e-6);
        assert_eq!(with("Split Beam", -4.0, 0.0).projectile_count, 1);
        // Increase Projectile Speed: value A is the multiplier; the figures stay.
        let fast = with("Increase Projectile Speed", 1.5, 0.0);
        assert_eq!((fast.base_damage, fast.projectile_speed_mult), (10, Some(1.5)));
        assert_eq!(base.projectile_speed_mult, None);
        // Regenerate Ammo (shots): every fourth shot free; (seconds): on a recharging
        // weapon, added to its Regen Rate, never below 0.1 s.
        assert!(with("Regenerate Ammo (shots)", 0.0, 0.0).free_fourth_shot && !base.free_fourth_shot);
        let recharger = WeaponStats { regen_rate: 2.0, ..base.clone() };
        let mut r = recharger.clone();
        r.mods = vec![slot(1, 1, "Regenerate Ammo (seconds)", -0.5, 0.0)];
        assert!((r.modded(1).regen_rate - 1.5).abs() < 1e-6);
        r.mods = vec![slot(1, 1, "Regenerate Ammo (seconds)", -5.0, 0.0)];
        assert!((r.modded(1).regen_rate - 0.1).abs() < 1e-6);
        assert_eq!(with("Regenerate Ammo (seconds)", 3.0, 0.0).regen_rate, 0.0);
    }
}
