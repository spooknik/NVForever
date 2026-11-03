//! The effect content of shots (docs/spec-notes/combat-rules.md §16): what a weapon, a
//! projectile, an explosion and an impact sound and look like. Read with the record
//! layouts in `schemas/fnv` (WEAP, SOUN and TXST in `objects.json`; PROJ and EXPL in
//! `combat.json`; ADDN, IPCT and IPDS in `placed.json`; DODT in `common.json`), all
//! [documented: xEdit]. This
//! module keeps the records' own names; what the engine does with them is its business.
//!
//! Kept apart from [`crate::data::CombatForms`]: the rules do not need any of it.

use std::collections::HashMap;

use fnv_esm::{LoadOrder, Record, Schema};
use fnv_sim::extract::{self, all, first, global, has_flag, int, local_fid, num, text, Decoded};
use fnv_sim::FormId;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;

/// SOUN.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Sound {
    pub edid: Option<String>,
    /// `FNAM`: a file or a folder of files under `sound\`.
    pub file: Option<String>,
    /// `SNDD` flag names (`Loop`, `2D`, `Random Frequency Shift`, …).
    pub flags: Vec<String>,
    /// Game units: the stored bytes times 5 and times 100 [documented: xEdit].
    pub min_distance: f32,
    pub max_distance: f32,
    /// Decibels taken off (the stored value is in hundredths).
    pub static_attenuation_db: f32,
    pub frequency_adjust_percent: f32,
    /// `RNAM` "Random Chance %".
    pub random_chance_percent: f32,
    /// `SNDD` start and stop time in hours: the stored byte over 256 is the fraction of
    /// a day [documented: xEdit wbAlocTime].
    pub start_hour: f32,
    pub stop_hour: f32,
    /// `SNDD` "Attenuation Curve": five volume percentages from the minimum to the
    /// maximum attenuation distance ("the topmost slider is the sound's volume
    /// percentage at its inner radius. The bottommost ... at its outer radius"
    /// [documented: GECK wiki Sound, Attenuation tab]); `None` when the record has none
    /// (an `SNDX`, or a sound built by hand), which plays the default curve
    /// ([`DEFAULT_ATTENUATION_CURVE`]). Not part of any report.
    #[serde(skip)]
    pub attenuation_curve: Option<[f32; 5]>,
    /// `SNDD` "Reverb Attenuation Control", percent: 100 sends nothing into the reverb, 0
    /// all of it [documented: GECK wiki Sound]. Not part of any report.
    #[serde(skip)]
    pub reverb_attenuation_percent: f32,
}

/// The curve the GECK starts a sound with, "a logarithmic falloff" [documented: GECK wiki
/// Sound]: 100, 50, 20, 5, 0 [observed: 2,423 of FalloutNV.esm's 3,190 sounds store it].
pub const DEFAULT_ATTENUATION_CURVE: [f32; 5] = [100.0, 50.0, 20.0, 5.0, 0.0];

impl Sound {
    pub fn has(&self, flag: &str) -> bool {
        self.flags.iter().any(|f| f == flag)
    }
}

/// The sounds and the shell casing of a WEAP.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WeaponFx {
    /// `SNAM`, first and second: Shoot 3D and Shoot Dist.
    pub shoot_3d: Option<FormId>,
    pub shoot_dist: Option<FormId>,
    /// `XNAM`: Shoot 2D.
    pub shoot_2d: Option<FormId>,
    /// `NAM7`: Shoot 3D Looping.
    pub shoot_loop: Option<FormId>,
    /// `WMS1`, first and second: Mod 1 Shoot 3D and Shoot Dist; `WMS2`: Mod 1 Shoot 2D.
    /// Played instead of the plain ones while a Silence or Suppressor mod is on
    /// (docs/spec-notes/weapon-mods.md §7).
    pub mod_shoot_3d: Option<FormId>,
    pub mod_shoot_dist: Option<FormId>,
    pub mod_shoot_2d: Option<FormId>,
    /// `TNAM`: Melee Swing / Gun No Ammo.
    pub no_ammo: Option<FormId>,
    /// `NAM9`, `NAM8`.
    pub equip: Option<FormId>,
    pub unequip: Option<FormId>,
    /// `NAM6` Sound - Block and `UNAM` Sound - Idle [documented: xEdit
    /// wbDefinitionsFNV.pas WEAP] (docs/spec-notes/audio.md §15).
    pub block: Option<FormId>,
    pub idle: Option<FormId>,
    /// `MOD2`: the shell casing model.
    pub shell_casing_model: Option<String>,
    /// `INAM`.
    pub impact_dataset: Option<FormId>,
}

/// The shoot sounds a weapon unit plays: the plain set or the mod set
/// ([`WeaponFx::shoot_sounds`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShootSounds {
    /// Heard by the one firing (Shoot 2D, else Shoot 3D).
    pub own: Option<FormId>,
    /// Heard from another actor's shot, within the 3D sound's maximum distance.
    pub near: Option<FormId>,
    /// Heard from another actor's shot farther off than that.
    pub dist: Option<FormId>,
    /// The mod set was chosen.
    pub silenced: bool,
}

impl ShootSounds {
    /// The sound of another actor's shot `distance` units from the listener, given the
    /// 3D sound's maximum attenuation distance (`near_max`, 0 when unknown): the distant
    /// sound at or beyond it [documented: GECK wiki Weapons, Attack Sound (Dist)], else
    /// the 3D one. Beyond it with no distant sound: the 3D one, which is then inaudible.
    pub fn heard_at(&self, distance: f32, near_max: f32) -> (Option<FormId>, bool) {
        match self.dist {
            Some(d) if near_max > 0.0 && distance >= near_max => (Some(d), true),
            _ => (self.near.or(self.own), false),
        }
    }
}

impl WeaponFx {
    /// The shoot sounds of a unit with (`silenced`) or without a Silence or Suppressor mod
    /// on (docs/spec-notes/weapon-mods.md §7): the `WMS1`/`WMS2` set when silenced and
    /// the record has one, else the plain `SNAM`/`XNAM` set. A missing 2D falls back to
    /// the 3D and the reverse [hypothesis]; the silenced set's distant sound is only its
    /// own (`WMS1` second), never the plain one [hypothesis].
    pub fn shoot_sounds(&self, silenced: bool) -> ShootSounds {
        let modded = silenced && (self.mod_shoot_3d.is_some() || self.mod_shoot_2d.is_some());
        let (d3, dist, d2) = if modded { (self.mod_shoot_3d, self.mod_shoot_dist, self.mod_shoot_2d) } else { (self.shoot_3d, self.shoot_dist, self.shoot_2d) };
        ShootSounds { own: d2.or(d3), near: d3.or(d2), dist, silenced: modded }
    }
}

/// The sounds of a PROJ and of an EXPL.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ProjectileFx {
    pub sound: Option<FormId>,
    /// `DATA` sound_countdown: a mine's tick while its fuse burns (`WPNMineTick` for the
    /// frag mine) [documented: xEdit PROJ; observed: the frag mine's names it].
    pub sound_countdown: Option<FormId>,
    /// `DATA` sound_disable: a mine being disarmed (`WPNMineDisarm`).
    pub sound_disable: Option<FormId>,
    /// `DATA` flag "Supersonic": "the sound emanates from the point of the projectile's
    /// path closest to the listener (Think bullets whizzing by your head). If unchecked,
    /// the sound emitter follows the projectile around until it is destroyed"
    /// [documented: GECK wiki Projectile]. Not part of any report.
    #[serde(skip)]
    pub supersonic: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ExplosionFx {
    pub model: Option<String>,
    /// `DATA` light (LIGH).
    pub light: Option<FormId>,
    pub sound_1: Option<FormId>,
    pub sound_2: Option<FormId>,
    pub impact_dataset: Option<FormId>,
}

/// ADDN: the model an `AddOnNode<index>` of a NIF stands for.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AddonNode {
    pub edid: Option<String>,
    pub index: i32,
    pub model: Option<String>,
    pub sound: Option<FormId>,
    pub particle_cap: u32,
}

/// IPCT.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Impact {
    pub edid: Option<String>,
    pub model: Option<String>,
    pub effect_duration: f32,
    /// Surface Normal, Projectile Vector or Projectile Reflection.
    pub orientation: Option<String>,
    pub texture_set: Option<FormId>,
    pub sound_1: Option<FormId>,
    pub sound_2: Option<FormId>,
    /// `DATA` angle threshold (degrees) and placement radius (units): "how far around in
    /// degrees a decal renders" and "the distance between decals" [documented: GECK wiki
    /// ImpactData].
    pub angle_threshold: f32,
    pub placement_radius: f32,
    /// `DATA` sound level ("Loud", "Normal", "Silent") [documented: xEdit IPCT]: the
    /// loudness of the impact for detection (docs/design/detection.md §8).
    pub sound_level: Option<String>,
    /// `DODT`: the impact's own decal data, which "overrides the Texture Set decal
    /// data" [documented: GECK wiki ImpactData]; `None` with the `DATA` flag "No Decal
    /// Data", when the texture set's is used (docs/spec-notes/impact-decals.md §1).
    pub decal: Option<DecalData>,
}

/// `DODT` Decal Data, of an IPCT or a TXST [documented: xEdit wbDODT; GECK wiki
/// ImpactData]. Sizes and depth in game units.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct DecalData {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
    /// "How far from the point of impact the decal renders".
    pub depth: f32,
    /// "Sets the specular exponent".
    pub shininess: f32,
    pub parallax: bool,
    pub parallax_scale: f32,
    pub parallax_passes: u8,
    pub alpha_blending: bool,
    pub alpha_testing: bool,
    /// "Multiplies the texture by the selected color".
    pub color: [u8; 3],
}

impl DecalData {
    /// The side of a decal for `r` in 0..1: between the smaller and the larger of the
    /// width range (a record may store them the other way round: `FlamerScorchStoneImpact`
    /// 120 and 80). The height range is not used: the decal is square
    /// (docs/spec-notes/impact-decals.md §2) [design].
    pub fn side(&self, r: f32) -> f32 {
        let (lo, hi) = (self.min_width.min(self.max_width), self.min_width.max(self.max_width));
        (lo + (hi - lo) * r.clamp(0.0, 1.0)).max(0.0)
    }
}

/// TXST: the textures of a texture set (relative to `textures\`) and its decal data.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TextureSet {
    pub edid: Option<String>,
    /// `TX00` Base Image / Transparency, `TX01` Normal Map / Specular, `TX04` Parallax Map.
    pub diffuse: Option<String>,
    pub normal: Option<String>,
    pub parallax: Option<String>,
    /// `DNAM` flag "No Specular Map".
    pub no_specular: bool,
    pub decal: Option<DecalData>,
}

/// The materials of an IPDS, in the record's order.
pub const IMPACT_MATERIALS: [&str; 12] =
    ["stone", "dirt", "grass", "glass", "metal", "wood", "organic", "cloth", "water", "hollow_metal", "organic_bug", "organic_glow"];

/// The effect forms of a load order.
#[derive(Debug, Default)]
pub struct FxForms {
    pub sounds: HashMap<FormId, Sound>,
    pub weapons: HashMap<FormId, WeaponFx>,
    pub projectiles: HashMap<FormId, ProjectileFx>,
    pub explosions: HashMap<FormId, ExplosionFx>,
    /// By node index (`DATA`).
    pub addons: HashMap<i32, AddonNode>,
    pub impacts: HashMap<FormId, Impact>,
    /// TXST.
    pub texture_sets: HashMap<FormId, TextureSet>,
    /// IPDS: the IPCT of each of [`IMPACT_MATERIALS`].
    pub impact_sets: HashMap<FormId, [Option<FormId>; 12]>,
    /// NPC_ and CREA `NAM4` Impact Material Type, an index into [`IMPACT_MATERIALS`]
    /// (the enumeration lists the same twelve in the same order) [documented: xEdit].
    pub actor_impact: HashMap<FormId, usize>,
    /// CREA CNAM: each creature's attack impact data set ([`FxForms::creature_impact_set`]).
    pub creature_impacts: HashMap<FormId, FormId>,
}

const SIGS: &[&[u8; 4]] = &[b"SOUN", b"WEAP", b"PROJ", b"EXPL", b"ADDN", b"IPCT", b"IPDS", b"TXST"];

enum Loaded {
    Sound(Sound),
    Weapon(WeaponFx),
    Projectile(ProjectileFx),
    Explosion(ExplosionFx),
    Addon(AddonNode),
    Impact(Impact),
    ImpactSet([Option<FormId>; 12]),
    TextureSet(TextureSet),
}

/// A decoded `DODT`.
fn decal_data(v: &Value) -> DecalData {
    let n = |key: &str| num(v, key).unwrap_or(0.0) as f32;
    let flags = v.get("flags");
    let flag = |name: &str| flags.is_some_and(|f| has_flag(f, name));
    let c = v.get("color");
    let channel = |key: &str| c.and_then(|c| int(c, key)).unwrap_or(255).clamp(0, 255) as u8;
    DecalData {
        min_width: n("min_width"),
        max_width: n("max_width"),
        min_height: n("min_height"),
        max_height: n("max_height"),
        depth: n("depth"),
        shininess: n("shininess"),
        parallax: flag("Parallax"),
        parallax_scale: n("parallax_scale"),
        parallax_passes: int(v, "parallax_passes").unwrap_or(0).clamp(0, 255) as u8,
        alpha_blending: flag("Alpha - Blending"),
        alpha_testing: flag("Alpha - Testing"),
        color: [channel("red"), channel("green"), channel("blue")],
    }
}

fn path(v: Option<&Value>) -> Option<String> {
    v.and_then(text).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// A subrecord's bytes as a string (for a subrecord the record's layout does not list).
fn raw_string(r: &Record, sig: &[u8; 4]) -> Option<String> {
    let subs = r.subrecords().ok()?;
    let s = subs.iter().find(|s| &s.sig == sig)?;
    let t = fnv_util::text::decode_z(&s.data).trim().to_string();
    (!t.is_empty()).then_some(t)
}

impl FxForms {
    pub fn build(lo: &LoadOrder, schema: &Schema) -> FxForms {
        let mut wanted: Vec<(FormId, usize, &Record)> = Vec::new();
        for (&g, defs) in &lo.index {
            let Some(d) = defs.last() else { continue };
            let r = lo.records[d.plugin as usize][d.ordinal as usize];
            if SIGS.contains(&&r.header.sig) && r.header.flags & 0x20 == 0 {
                wanted.push((FormId(g), d.plugin as usize, r));
            }
        }
        wanted.sort_by_key(|w| w.0);
        let loaded: Vec<(FormId, Loaded)> = wanted
            .par_iter()
            .filter_map(|&(id, pi, r)| {
                let d = extract::decode(r, schema).ok()?;
                // A FormID of this record as a global one; null and unresolved: `None`.
                let fid = |v: Option<&Value>| v.and_then(local_fid).and_then(|l| global(lo, pi, l).ok().flatten());
                let field = |v: Option<&Value>, key: &str| fid(v.and_then(|v| v.get(key)));
                let edid = first(&d, b"EDID").and_then(text);
                Some((id, load_one(r, &d, edid, &fid, &field)?))
            })
            .collect();
        let mut out = FxForms { actor_impact: actor_impact_materials(lo), creature_impacts: creature_impact_sets(lo), ..FxForms::default() };
        for (id, l) in loaded {
            match l {
                Loaded::Sound(s) => {
                    out.sounds.insert(id, s);
                }
                Loaded::Weapon(w) => {
                    out.weapons.insert(id, w);
                }
                Loaded::Projectile(p) => {
                    out.projectiles.insert(id, p);
                }
                Loaded::Explosion(e) => {
                    out.explosions.insert(id, e);
                }
                Loaded::Addon(a) => {
                    out.addons.insert(a.index, a);
                }
                Loaded::Impact(i) => {
                    out.impacts.insert(id, i);
                }
                Loaded::ImpactSet(s) => {
                    out.impact_sets.insert(id, s);
                }
                Loaded::TextureSet(t) => {
                    out.texture_sets.insert(id, t);
                }
            }
        }
        out
    }

    /// The IPCT of `set` for a material of [`IMPACT_MATERIALS`].
    pub fn impact(&self, set: FormId, material: &str) -> Option<&Impact> {
        self.impacts.get(&self.impact_id(set, material)?)
    }

    /// The impact data set of a creature's own attacks (CREA `CNAM` Impact Dataset
    /// [documented: xEdit]; `BiteDataSet`, `ClawDataSet` … [observed]).
    pub fn creature_impact_set(&self, base: FormId) -> Option<FormId> {
        self.creature_impacts.get(&base).copied()
    }

    /// The impact material of an actor whose base is `base` (NPC_ or CREA `NAM4`).
    pub fn actor_material(&self, base: FormId) -> Option<&'static str> {
        self.actor_impact.get(&base).and_then(|&k| IMPACT_MATERIALS.get(k)).copied()
    }

    /// The decal an impact leaves: its texture set and the decal data that holds, the
    /// impact's own, else the texture set's (docs/spec-notes/impact-decals.md §1). `None`
    /// without a texture set with a base image, or without decal data.
    pub fn decal(&self, impact: &Impact) -> Option<(&TextureSet, DecalData)> {
        let t = self.texture_sets.get(&impact.texture_set?)?;
        t.diffuse.as_ref()?;
        Some((t, impact.decal.or(t.decal)?))
    }

    pub fn impact_id(&self, set: FormId, material: &str) -> Option<FormId> {
        let k = IMPACT_MATERIALS.iter().position(|m| *m == material)?;
        self.impact_sets.get(&set)?[k]
    }
}

/// `NAM4` of every NPC_ and CREA, read from the raw subrecord (a `u32` enumeration,
/// [documented: xEdit]); values outside the twelve are left out.
fn actor_impact_materials(lo: &LoadOrder) -> HashMap<FormId, usize> {
    let actors: Vec<(FormId, &Record)> = lo
        .index
        .iter()
        .filter_map(|(&g, defs)| {
            let d = defs.last()?;
            let r = lo.records[d.plugin as usize][d.ordinal as usize];
            (matches!(&r.header.sig, b"NPC_" | b"CREA") && r.header.flags & 0x20 == 0).then_some((FormId(g), r))
        })
        .collect();
    actors
        .par_iter()
        .filter_map(|&(id, r)| {
            let subs = r.subrecords().ok()?;
            let s = subs.iter().find(|s| &s.sig == b"NAM4")?;
            let v = u32::from_le_bytes(s.data.get(..4)?.try_into().ok()?) as usize;
            (v < IMPACT_MATERIALS.len()).then_some((id, v))
        })
        .collect()
}

/// CREA `CNAM` (Impact Dataset) of every creature that names one, read raw as `NAM4` is.
fn creature_impact_sets(lo: &LoadOrder) -> HashMap<FormId, FormId> {
    let creatures: Vec<(FormId, usize, &Record)> = lo
        .index
        .iter()
        .filter_map(|(&g, defs)| {
            let d = defs.last()?;
            let r = lo.records[d.plugin as usize][d.ordinal as usize];
            (&r.header.sig == b"CREA" && r.header.flags & 0x20 == 0).then_some((FormId(g), d.plugin as usize, r))
        })
        .collect();
    creatures
        .par_iter()
        .filter_map(|&(id, pi, r)| {
            let subs = r.subrecords().ok()?;
            let s = subs.iter().find(|s| &s.sig == b"CNAM")?;
            let local = u32::from_le_bytes(s.data.get(..4)?.try_into().ok()?);
            (local != 0).then_some(())?;
            Some((id, global(lo, pi, local).ok().flatten()?))
        })
        .collect()
}

fn load_one(r: &Record, d: &[Decoded], edid: Option<String>, fid: &dyn Fn(Option<&Value>) -> Option<FormId>, field: &dyn Fn(Option<&Value>, &str) -> Option<FormId>) -> Option<Loaded> {
    Some(match &r.header.sig {
        b"SOUN" => {
            // `SNDD`, or the older `SNDX` with the same first fields.
            let v = first(d, b"SNDD").or_else(|| first(d, b"SNDX"));
            let n = |key: &str| v.and_then(|v| num(v, key)).unwrap_or(0.0) as f32;
            let flags = v.and_then(|v| v.get("flags"));
            Loaded::Sound(Sound {
                edid,
                file: path(first(d, b"FNAM")),
                flags: flags.and_then(|f| f.get("set")).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default(),
                min_distance: n("min_attenuation_distance") * 5.0,
                max_distance: n("max_attenuation_distance") * 100.0,
                static_attenuation_db: n("static_attenuation_cdb") / 100.0,
                frequency_adjust_percent: n("frequency_adjustment_percent"),
                random_chance_percent: first(d, b"RNAM").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                start_hour: n("start_time") / 256.0 * 24.0,
                stop_hour: n("stop_time") / 256.0 * 24.0,
                attenuation_curve: v.and_then(|v| v.get("attenuation_curve")).and_then(Value::as_array).and_then(|a| {
                    let p: Vec<f32> = a.iter().filter_map(Value::as_f64).map(|x| x as f32).collect();
                    <[f32; 5]>::try_from(p).ok()
                }),
                reverb_attenuation_percent: n("reverb_attenuation_control"),
            })
        }
        b"WEAP" => {
            let shoot: Vec<&Value> = all(d, b"SNAM").collect();
            let mod_shoot: Vec<&Value> = all(d, b"WMS1").collect();
            Loaded::Weapon(WeaponFx {
                shoot_3d: fid(shoot.first().copied()),
                shoot_dist: fid(shoot.get(1).copied()),
                shoot_2d: fid(first(d, b"XNAM")),
                shoot_loop: fid(first(d, b"NAM7")),
                mod_shoot_3d: fid(mod_shoot.first().copied()),
                mod_shoot_dist: fid(mod_shoot.get(1).copied()),
                mod_shoot_2d: fid(first(d, b"WMS2")),
                no_ammo: fid(first(d, b"TNAM")),
                equip: fid(first(d, b"NAM9")),
                unequip: fid(first(d, b"NAM8")),
                block: fid(first(d, b"NAM6")),
                idle: fid(first(d, b"UNAM")),
                shell_casing_model: path(first(d, b"MOD2")).or_else(|| raw_string(r, b"MOD2")),
                impact_dataset: fid(first(d, b"INAM")),
            })
        }
        b"PROJ" => {
            let v = first(d, b"DATA");
            let supersonic = v.and_then(|v| v.get("flags")).is_some_and(|f| has_flag(f, "Supersonic"));
            Loaded::Projectile(ProjectileFx { sound: field(v, "sound"), sound_countdown: field(v, "sound_countdown"), sound_disable: field(v, "sound_disable"), supersonic })
        }
        b"EXPL" => {
            let v = first(d, b"DATA");
            Loaded::Explosion(ExplosionFx {
                model: path(first(d, b"MODL")),
                light: field(v, "light"),
                sound_1: field(v, "sound_1"),
                sound_2: field(v, "sound_2"),
                impact_dataset: field(v, "impact_dataset"),
            })
        }
        b"ADDN" => Loaded::Addon(AddonNode {
            edid,
            index: first(d, b"DATA").and_then(Value::as_i64).unwrap_or(-1) as i32,
            model: path(first(d, b"MODL")),
            sound: fid(first(d, b"SNAM")),
            particle_cap: first(d, b"DNAM").and_then(|v| int(v, "master_particle_system_cap")).unwrap_or(0).max(0) as u32,
        }),
        b"IPCT" => {
            let v = first(d, b"DATA");
            Loaded::Impact(Impact {
                edid,
                model: path(first(d, b"MODL")),
                effect_duration: v.and_then(|v| num(v, "effect_duration")).unwrap_or(0.0) as f32,
                orientation: v.and_then(|v| v.get("effect_orientation")).and_then(|x| match x {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                }),
                texture_set: fid(first(d, b"DNAM")),
                sound_1: fid(first(d, b"SNAM")),
                sound_2: fid(first(d, b"NAM1")),
                angle_threshold: v.and_then(|v| num(v, "angle_threshold")).unwrap_or(0.0) as f32,
                placement_radius: v.and_then(|v| num(v, "placement_radius")).unwrap_or(0.0) as f32,
                sound_level: v.and_then(|v| v.get("sound_level")).and_then(|x| match x {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                }),
                decal: if v.and_then(|v| v.get("flags")).is_some_and(|f| has_flag(f, "No Decal Data")) { None } else { first(d, b"DODT").map(decal_data) },
            })
        }
        b"TXST" => Loaded::TextureSet(TextureSet {
            edid,
            diffuse: path(first(d, b"TX00")),
            normal: path(first(d, b"TX01")),
            parallax: path(first(d, b"TX04")),
            no_specular: first(d, b"DNAM").is_some_and(|f| has_flag(f, "No Specular Map")),
            decal: first(d, b"DODT").map(decal_data),
        }),
        b"IPDS" => {
            let v = first(d, b"DATA");
            Loaded::ImpactSet(std::array::from_fn(|k| field(v, IMPACT_MATERIALS[k])))
        }
        _ => return None,
    })
}

/// The material of [`IMPACT_MATERIALS`] a collision material stands for.
///
/// The collision codes are nif.xml's `Fallout3HavokMaterial` [documented]: 32 materials,
/// then the same again as platforms, stairs and stair platforms (code mod 32). Which of
/// the twelve impact materials each one takes is ours [hypothesis, from the names]: the
/// record has no table for it. No code (terrain) counts as dirt [design].
pub fn impact_material(code: Option<u32>) -> &'static str {
    let Some(code) = code else { return "dirt" };
    match code % 32 {
        0 | 10 | 19 => "stone",
        1 => "cloth",
        2 | 18 => "dirt",
        3 | 24 => "glass",
        4 => "grass",
        6 | 7 => "organic",
        8 => "water",
        9 | 12 => "wood",
        // Hollow, sheet, vehicle bodies and hollow parts, barrels, cans, carts, boxes.
        16 | 17 | 20 | 22 | 23 | 25 | 28 | 29 => "hollow_metal",
        // Metal, heavy metal, chain, caps, elevators, solid parts, guns, and the rest.
        _ => "metal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_materials_to_impact_materials() {
        assert_eq!(impact_material(None), "dirt");
        assert_eq!(impact_material(Some(0)), "stone");
        assert_eq!(impact_material(Some(9)), "wood");
        assert_eq!(impact_material(Some(5)), "metal");
        assert_eq!(impact_material(Some(16)), "hollow_metal");
        // Wood stairs (73) and a glass platform (35) are wood and glass.
        assert_eq!(impact_material(Some(73)), "wood");
        assert_eq!(impact_material(Some(35)), "glass");
        for code in 0..128 {
            assert!(IMPACT_MATERIALS.contains(&impact_material(Some(code))));
        }
    }

    #[test]
    fn silenced_units_shoot_with_the_mod_set() {
        let f = |x: u32| Some(FormId(x));
        // The 10mm pistol's shape: a plain pair and 2D, one WMS1 and a WMS2.
        let w = WeaponFx { shoot_3d: f(1), shoot_dist: f(2), shoot_2d: f(3), mod_shoot_3d: f(4), mod_shoot_2d: f(5), ..WeaponFx::default() };
        assert_eq!(w.shoot_sounds(false), ShootSounds { own: f(3), near: f(1), dist: f(2), silenced: false });
        assert_eq!(w.shoot_sounds(true), ShootSounds { own: f(5), near: f(4), dist: None, silenced: true });
        // Far off: the plain shot's distant boom, the silenced shot's own 3D (unheard).
        assert_eq!(w.shoot_sounds(false).heard_at(5000.0, 4000.0), (f(2), true));
        assert_eq!(w.shoot_sounds(false).heard_at(3000.0, 4000.0), (f(1), false));
        assert_eq!(w.shoot_sounds(true).heard_at(5000.0, 4000.0), (f(4), false));
        // No mod set (the silenced .22 pistol carries its quiet sound in SNAM): plain.
        let plain = WeaponFx { shoot_3d: f(1), ..WeaponFx::default() };
        assert_eq!(plain.shoot_sounds(true), ShootSounds { own: f(1), near: f(1), dist: None, silenced: false });
        // Only a mod 3D: the player hears that too.
        let only_3d = WeaponFx { shoot_3d: f(1), mod_shoot_3d: f(4), ..WeaponFx::default() };
        assert_eq!(only_3d.shoot_sounds(true).own, f(4));
    }

    #[test]
    fn decal_side_spans_the_width_range_either_way_round() {
        let d = DecalData { min_width: 120.0, max_width: 80.0, min_height: 8.0, max_height: 32.0, ..DecalData::default() };
        assert_eq!(d.side(0.0), 80.0);
        assert_eq!(d.side(1.0), 120.0);
        assert_eq!(d.side(0.5), 100.0);
        assert_eq!(d.side(2.0), 120.0);
    }

    #[test]
    fn an_impact_without_decal_data_of_its_own_takes_the_texture_set_one() {
        let own = DecalData { min_width: 10.0, max_width: 20.0, ..DecalData::default() };
        let set = DecalData { min_width: 2.0, max_width: 2.0, ..DecalData::default() };
        let mut f = FxForms::default();
        f.texture_sets.insert(FormId(1), TextureSet { diffuse: Some("Decals\\A.dds".into()), decal: Some(set), ..TextureSet::default() });
        f.texture_sets.insert(FormId(2), TextureSet { diffuse: None, decal: Some(set), ..TextureSet::default() });
        let mut i = Impact { texture_set: Some(FormId(1)), decal: Some(own), ..Impact::default() };
        assert_eq!(f.decal(&i).map(|d| d.1), Some(own));
        i.decal = None;
        assert_eq!(f.decal(&i).map(|d| d.1), Some(set));
        // No base image, or no texture set: nothing to draw.
        i.texture_set = Some(FormId(2));
        assert!(f.decal(&i).is_none());
        i.texture_set = None;
        assert!(f.decal(&i).is_none());
    }
}
