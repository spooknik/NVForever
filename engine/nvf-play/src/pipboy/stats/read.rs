//! Reading what the STATS page shows from the game: the records' words and pictures
//! ([`Forms`], read once each from the plugins) and the player's figures
//! ([`snapshot`], from the scripts' game and the simulation's world).

use super::model::{self, Change, EffectLine, Hints, Row, Snapshot, Standing, LIMBS};
use crate::script::ScriptState;
use crate::sim::SimState;
use fnv_esm::{LoadOrder, Schema};
use fnv_game::actor_values::{self as av, SKILLS, SPECIALS};
use fnv_game::functions as gf;
use fnv_sim::FormId;
use fnv_world::fields::{formid_of, global, winner, Fields};
use std::collections::HashMap;

/// A record's words and picture.
#[derive(Clone, Debug, Default)]
pub struct Text {
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    /// AVIF ANAM: the short name (`STR`, `Melee Weap.`) [observed: a zero-ended text in
    /// every AVIF; the EFF screenshot writes "Melee Weap." for Melee Weapons].
    pub short: Option<String>,
}

/// A perk's DATA: trait, minimum level, ranks, playable, hidden [hypothesis: the five
/// bytes as xEdit names them; `IntenseTraining`'s are 0, 2, 10, 1, 0 and it has ten
/// ranks].
#[derive(Clone, Copy, Debug, Default)]
pub struct PerkData {
    pub ranks: u8,
    pub playable: bool,
    pub hidden: bool,
}

/// An enchantment (ENCH): its name, whether it is worn apparel's, and its value changes
/// (actor value, magnitude, detrimental).
#[derive(Clone, Debug, Default)]
pub struct Ench {
    pub name: String,
    pub apparel: bool,
    pub changes: Vec<(u16, f64, bool)>,
}

/// The records read so far.
pub struct Forms {
    lo: &'static LoadOrder<'static>,
    schema: &'static Schema,
    texts: HashMap<u32, Text>,
    perks: HashMap<u32, PerkData>,
    enchs: HashMap<u32, Ench>,
    /// MGEF: (archetype is Value Modifier, flag Detrimental).
    effects: HashMap<u32, (bool, bool)>,
    /// An item's enchantment (EITM).
    item_ench: HashMap<u32, Option<u32>>,
    /// The hints' items from the DOBJ list ([`Forms::hint_items`]), read once.
    hint_items: Option<[Option<u32>; 4]>,
}

fn fields(lo: &'static LoadOrder<'static>, schema: &'static Schema, id: u32) -> Option<(usize, Fields<'static>)> {
    let (pi, rec) = winner(lo, id)?;
    Some((pi, Fields::new(rec, pi, schema).ok()?))
}

/// A zero-ended text of single-byte characters.
fn zstring(b: &[u8]) -> String {
    b.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect()
}

impl Forms {
    pub fn new(lo: &'static LoadOrder<'static>, schema: &'static Schema) -> Forms {
        Forms { lo, schema, texts: HashMap::new(), perks: HashMap::new(), enchs: HashMap::new(), effects: HashMap::new(), item_ench: HashMap::new(), hint_items: None }
    }

    /// FULL, DESC, ICON (and ANAM) of a record.
    pub fn text(&mut self, id: u32) -> &Text {
        let (lo, schema) = (self.lo, self.schema);
        self.texts.entry(id).or_insert_with(|| {
            let Some((_, f)) = fields(lo, schema, id) else { return Text::default() };
            Text {
                name: f.string(b"FULL").unwrap_or_default(),
                description: f.string(b"DESC").unwrap_or_default().trim().to_string(),
                icon: f.string(b"ICON").filter(|s| !s.trim().is_empty()),
                short: f.raw(b"ANAM").map(|s| zstring(&s.data)).filter(|s| !s.is_empty()),
            }
        })
    }

    /// The words of an actor value (its AVIF record).
    pub fn value(&mut self, code: u16) -> Text {
        let t = model::avif_form(code).map(|id| self.text(id).clone()).unwrap_or_default();
        if t.name.is_empty() {
            Text { name: gf::AV_NAMES.get(code as usize).map_or_else(String::new, |n| n.to_string()), ..t }
        } else {
            t
        }
    }

    pub fn perk(&mut self, id: u32) -> PerkData {
        let (lo, schema) = (self.lo, self.schema);
        *self.perks.entry(id).or_insert_with(|| {
            let d = fields(lo, schema, id).and_then(|(_, f)| f.raw(b"DATA").map(|s| s.data.to_vec())).unwrap_or_default();
            PerkData { ranks: d.get(2).copied().unwrap_or(1), playable: d.get(3).is_none_or(|&b| b != 0), hidden: d.get(4).is_some_and(|&b| b != 0) }
        })
    }

    fn effect(&mut self, id: u32) -> (bool, bool) {
        let (lo, schema) = (self.lo, self.schema);
        *self.effects.entry(id).or_insert_with(|| {
            let Some(d) = fields(lo, schema, id).and_then(|(_, f)| f.get(b"DATA")) else { return (false, false) };
            let modifier = d.get("archetype").and_then(|a| a.as_str()) == Some("Value Modifier");
            let detrimental = d.get("flags").and_then(|f| f.get("set")).and_then(|s| s.as_array()).is_some_and(|s| s.iter().any(|x| x.as_str() == Some("Detrimental")));
            (modifier, detrimental)
        })
    }

    /// An enchantment's value changes: each EFID's base effect when it is a Value
    /// Modifier [hypothesis: only those change a value the page shows], with the EFIT's
    /// magnitude and actor value [documented: schema `common.json` EFIT].
    pub fn ench(&mut self, id: u32) -> Ench {
        if let Some(e) = self.enchs.get(&id) {
            return e.clone();
        }
        let mut out = Ench::default();
        if let Some((pi, f)) = fields(self.lo, self.schema, id) {
            out.name = f.string(b"FULL").unwrap_or_default();
            out.apparel = f.get(b"ENIT").and_then(|v| v.get("type").and_then(|t| t.as_str().map(|t| t == "Apparel"))).unwrap_or(false);
            let mut base = None;
            for s in &f.subs {
                match &s.sig {
                    b"EFID" => base = f.decode(s).as_ref().and_then(formid_of).and_then(|l| global(self.lo, pi, l)),
                    b"EFIT" => {
                        let (Some(b), Some(v)) = (base.take(), f.decode(s)) else { continue };
                        let code = v.get("actor_value").and_then(|a| a.as_str()).and_then(gf::av_by_name);
                        let magnitude = v.get("magnitude").and_then(|m| m.as_f64()).unwrap_or(0.0);
                        let (modifier, detrimental) = self.effect(b);
                        if let (true, Some(code)) = (modifier, code) {
                            out.changes.push((code, magnitude, detrimental));
                        }
                    }
                    _ => {}
                }
            }
        }
        self.enchs.insert(id, out.clone());
        out
    }

    /// The hints' items: the DOBJ list's Stimpak, Doctor's Bag, RadAway and RadX
    /// ([`HINT_PLACES`]), each `None` when the list names none there.
    pub fn hint_items(&mut self) -> [Option<u32>; 4] {
        let lo = self.lo;
        *self.hint_items.get_or_insert_with(|| default_objects(lo, HINT_PLACES))
    }

    /// An item's enchantment (EITM) [observed: ARMO records carry it, e.g. the Vault 21
    /// Jumpsuit's `EnchClothingVaultSuitNormal`].
    pub fn item_ench(&mut self, item: u32) -> Option<u32> {
        let (lo, schema) = (self.lo, self.schema);
        *self.item_ench.entry(item).or_insert_with(|| fields(lo, schema, item).and_then(|(_, f)| f.formid(b"EITM", lo)))
    }
}

/// The items the status page's hints count, by Editor ID [observed: FalloutNV.esm ALCH
/// `Stimpak` 0x15169, `NVDoctorBag` 0xCB05C, `RadAway` 0x15167, `RadX` 0x15168]: the
/// fallback when the DOBJ list names none ([`HINT_PLACES`]).
const HINT_ITEMS: [&str; 4] = ["Stimpak", "NVDoctorBag", "RadAway", "RadX"];

/// The DOBJ places of the hints' items, in the hints' order: 0 Stimpak, 21 Doctor's Bag,
/// 3 RadAway, 2 RadX [documented: xEdit wbDefinitionsFNV.pas DOBJ names, in order;
/// docs/spec-notes/default-objects.md §1; observed: FalloutNV.esm names the four items of
/// [`HINT_ITEMS`] there].
const HINT_PLACES: [usize; 4] = [0, 21, 3, 2];

/// The default objects (DOBJ `DATA`, the winning definition of the last plugin that has
/// one) at `places`, as global FormIDs; `None` where the list names nothing.
fn default_objects(lo: &LoadOrder, places: [usize; 4]) -> [Option<u32>; 4] {
    let mut list: Vec<u32> = Vec::new();
    for (pi, records) in lo.records.iter().enumerate() {
        for r in records.iter().filter(|r| &r.header.sig == b"DOBJ" && r.header.flags & fnv_world::placed::ref_flags::DELETED == 0) {
            let Ok(subs) = r.subrecords() else { continue };
            if let Some(d) = subs.iter().find(|s| &s.sig == b"DATA") {
                list = d.data.as_chunks::<4>().0.iter().map(|c| global(lo, pi, u32::from_le_bytes(*c)).filter(|&x| x != 0).unwrap_or(0)).collect();
            }
        }
    }
    places.map(|i| list.get(i).copied().filter(|&x| x != 0))
}

/// The player's base form (NPC_ 0x7) [documented: GECK wiki, the player is 0x14 placed
/// of base 0x7].
const PLAYER_BASE: FormId = FormId(7);

/// Read everything the page shows. Without the scripts' game the figures it keeps are
/// left out ("--").
pub fn snapshot(forms: &mut Forms, sim: &mut SimState, script: Option<&mut ScriptState>) -> Snapshot {
    let mut s = Snapshot { name: sim.name(PLAYER_BASE).unwrap_or("").to_string(), limbs: [1.0; 6], ..Snapshot::default() };
    let (effects, temporary) = worn(forms, sim);
    s.effects = effects;
    // The hints' counts.
    let mut texts: [String; 4] = Default::default();
    let mut counts = [0; 4];
    let dobj = forms.hint_items();
    for (i, edid) in HINT_ITEMS.iter().enumerate() {
        let id = dobj[i].map(FormId).or_else(|| sim.forms.by_edid(edid));
        counts[i] = id.and_then(|id| sim.world.item_count(FormId::PLAYER_REF, id).ok()).unwrap_or(0);
        let name = id.and_then(|id| sim.name(id)).unwrap_or(edid).to_string();
        texts[i] = model::hint_text(&name, counts[i]);
    }
    s.hints = Hints { texts, counts, lit: [false; 4] };
    let xp_base = sim.forms.setting_f32("iXPBase").unwrap_or(200.0) as f64;
    let xp_bump = sim.forms.setting_f32("iXPBumpBase").unwrap_or(150.0) as f64;
    // The karma titles the plugins hold (`model::karma_title`).
    let karma_titles: std::collections::HashMap<String, String> = sim
        .forms
        .settings
        .iter()
        .filter_map(|(k, v)| match v {
            fnv_sim::settings::Setting::Str(t) if k.starts_with("skarmictitle") => Some((k.clone(), t.clone())),
            _ => None,
        })
        .collect();
    let karma_limits = ["KarmaVeryEvil", "KarmaEvil", "KarmaGood", "KarmaVeryGood"].map(|g| sim.forms.global_by_edid(g).map(|v| v.1 as f64));
    // The lists' words do not need the game.
    s.special = SPECIALS.map(|c| av_row(forms, c)).collect();
    let mut skills: Vec<Row> = SKILLS.filter(|&c| c != av::BIG_GUNS).map(|c| av_row(forms, c)).collect();
    s.general = model::MISC_STATS.iter().map(|n| Row { name: n.to_string(), value: Some("--".into()), ..Row::default() }).collect();
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else {
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        s.skills = skills;
        return s;
    };
    s.game = true;
    crate::script::with_world(rt, sim, |rt| {
        let g = &mut rt.game;
        let p = FormId::PLAYER_REF;
        let t = &temporary;
        let value = |g: &mut fnv_game::Game<'static>, code: u16| shown_and_permanent(g, t, code);
        let level = g.player_level();
        s.level = Some(level as i64);
        // The statistics' counters (`fnv_game::misc_stats`), by name.
        for row in &mut s.general {
            if let Some(code) = fnv_game::misc_stats::code(&row.name) {
                row.value = Some(format!("{}", g.misc_stat(code).round() as i64));
            }
        }
        // HP and AP out of a whole that an aid item's temporary modifier raises with them
        // (Buffout, Jet) [hypothesis].
        let whole = |g: &mut fnv_game::Game<'static>, code: u16| {
            let t = gf::actor_value_temporary(g, FormId::PLAYER_REF, code);
            value(g, code).map(|(v, max)| (v, max + t))
        };
        s.hp = whole(g, av::HEALTH);
        s.ap = whole(g, av::ACTION_POINTS);
        s.xp = Some((g.player.xp, model::xp_for_level(level + 1.0, xp_base, xp_bump)));
        for (i, &code) in LIMBS.iter().enumerate() {
            if let Some((v, max)) = value(g, code).filter(|v| v.1 > 0.0) {
                s.limbs[i] = (v / max).clamp(0.0, 1.0);
            }
        }
        // The CND hints lit while they would heal (`model::refusal` on the body).
        let hardcore = g.hardcore();
        for hint in [model::STIMPAK, model::DOCTORS_BAG] {
            let hp = s.hp.unwrap_or((0.0, 0.0));
            s.hints.lit[hint] = model::refusal(hint, None, s.hints.counts[hint], hp, &s.limbs, hardcore).is_none();
        }
        s.rad_resist = value(g, av::RAD_RESISTANCE).map(|v| v.0);
        s.rads = value(g, 54).map_or(0.0, |v| v.0.max(0.0));
        for (rows, codes) in [(&mut s.special, SPECIALS.collect::<Vec<_>>()), (&mut skills, SKILLS.filter(|&c| c != av::BIG_GUNS).collect())] {
            for (row, code) in rows.iter_mut().zip(codes) {
                if let Some((shown, permanent)) = value(g, code) {
                    let shown = av::capped(code, shown);
                    row.value = Some(model::whole(shown));
                    row.mark = model::modified_mark(shown, av::capped(code, permanent));
                }
            }
        }
        // Perks the player has, playable and not hidden [hypothesis: traits are listed
        // with them], by name; the rank when the perk has more than one.
        let mut perks: Vec<Row> = Vec::new();
        for (&perk, &rank) in &g.player.perks {
            let d = forms.perk(perk.0);
            if !d.playable || d.hidden {
                continue;
            }
            let t = forms.text(perk.0).clone();
            perks.push(Row { name: t.name, value: (d.ranks > 1).then(|| rank.to_string()), mark: None, description: t.description, icon: t.icon });
        }
        perks.sort_by(|a, b| a.name.cmp(&b.name));
        s.perks = perks;
        // Reputations with any fame or infamy [hypothesis: the list shows those].
        let mut standings: Vec<Standing> = Vec::new();
        for (&rep, &[infamy, fame]) in &g.player.reputation {
            if fame <= 0.0 && infamy <= 0.0 {
                continue;
            }
            let scale = g.data.reputations.get(&rep).copied().unwrap_or(0.0) as f64;
            let t = forms.text(rep.0).clone();
            let title = model::reputation_title(gf::reputation_level(fame, scale), gf::reputation_level(infamy, scale));
            standings.push(Standing { form: rep, name: t.name, title, fame, infamy, icon: t.icon });
        }
        standings.sort_by(|a, b| a.name.cmp(&b.name));
        s.standings = standings;
        if let (Some((karma, _)), [Some(ve), Some(e), Some(go), Some(vg)]) = (value(g, av::KARMA), karma_limits) {
            let (word, picture) = model::karma_alignment(karma, ve, e, go, vg);
            s.karma = Some((karma, word, picture));
            s.karma_title = Some(model::karma_title(level as i64, word, |n| karma_titles.get(&n.to_ascii_lowercase()).cloned()));
        }
        // Aid items taking effect now (ALCH sources of active effects), after the worn
        // apparel [hypothesis: the order]. An effect whose conditions fail (a Stimpak's
        // Hardcore heal outside Hardcore) is held inactive and changes nothing: not
        // listed (#1438).
        let mut seen: Vec<FormId> = Vec::new();
        for e in g.effects.iter().filter(|e| e.target == p && !e.inactive) {
            if seen.contains(&e.source) || g.data.sig(e.source) != Some(*b"ALCH") {
                continue;
            }
            seen.push(e.source);
            let changes: Vec<Change> = g
                .effects
                .iter()
                .filter(|x| x.target == p && x.source == e.source && !x.inactive)
                .filter_map(|x| {
                    let (modifier, detrimental) = forms.effect(x.effect.0);
                    let code = forms.ench_code(e.source.0, x.index)?;
                    // The amount the game applied (potency included, `fnv_game::aid`).
                    let amount = x.change.map_or(x.magnitude as f64, |c| c.amount.abs());
                    modifier.then(|| Change { value: short(forms, code), amount, detrimental })
                })
                .collect();
            if !changes.is_empty() {
                s.effects.push(EffectLine { name: forms.text(e.source.0).name.clone(), text: model::changes_text(&changes) });
            }
        }
    });
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    s.skills = skills;
    s
}

/// An actor value of the player: what the page shows (base, permanent modifier and
/// worn apparel's changes, less damage) and its permanent value (base and permanent
/// modifier) [design: worn enchantments do not change the game's actor values yet
/// (`fnv_game::functions::actor`, the temporary modifier); the page adds them so that
/// it reads as the original's].
/// The player's health (current and whole, as the heading), limb conditions (0 to 1, as
/// the CND bars) and whether Hardcore mode is on: what the CND page's hints are judged
/// by; `None` without the scripts' game.
pub fn condition(sim: &mut SimState, script: Option<&mut ScriptState>) -> Option<((f64, f64), [f64; 6], bool)> {
    let rt = script?.rt.as_mut()?;
    Some(crate::script::with_world(rt, sim, |rt| {
        let g = &mut rt.game;
        let t = gf::actor_value_temporary(g, FormId::PLAYER_REF, av::HEALTH);
        let hp = shown_and_permanent(g, &[], av::HEALTH).map_or((0.0, 0.0), |(v, max)| (v, max + t));
        let mut limbs = [1.0; 6];
        for (i, &code) in LIMBS.iter().enumerate() {
            if let Some((v, max)) = shown_and_permanent(g, &[], code).filter(|v| v.1 > 0.0) {
                limbs[i] = (v / max).clamp(0.0, 1.0);
            }
        }
        (hp, limbs, g.hardcore())
    }))
}

/// The hint's item (Stimpak, Doctor's Bag, RadAway, Rad-X), by its Editor ID.
pub fn hint_item(sim: &SimState, hint: usize) -> Option<FormId> {
    HINT_ITEMS.get(hint).and_then(|e| sim.forms.by_edid(e))
}

fn shown_and_permanent(g: &mut fnv_game::Game<'static>, temporary: &[(u16, f64, bool)], code: u16) -> Option<(f64, f64)> {
    let [b, m, d] = gf::actor_value_parts(g, FormId::PLAYER_REF, code).ok()?;
    // Active effects' temporary modifier (aid items) is the game's own; worn apparel's
    // is added here.
    let t = gf::actor_value_temporary(g, FormId::PLAYER_REF, code);
    Some((b + m + t - d + model::added(temporary, code), b + m))
}

/// What is worn: the enchantments of the player's equipped apparel, as the EFF list's
/// lines and as the changes they make.
fn worn(forms: &mut Forms, sim: &SimState) -> (Vec<EffectLine>, Vec<(u16, f64, bool)>) {
    let worn: Vec<FormId> = sim.world.peek(FormId::PLAYER_REF).and_then(|r| r.inventory.as_ref()).map(|i| i.equipped()).unwrap_or_default();
    let (mut lines, mut temporary) = (Vec::new(), Vec::new());
    for item in worn {
        if sim.forms.sig(item) != Some(*b"ARMO") {
            continue;
        }
        let Some(e) = forms.item_ench(item.0).map(|e| forms.ench(e)).filter(|e| e.apparel && !e.changes.is_empty()) else { continue };
        let changes: Vec<Change> = e.changes.iter().map(|&(code, amount, detrimental)| Change { value: short(forms, code), amount, detrimental }).collect();
        let name = if e.name.is_empty() { sim.name(item).unwrap_or("").to_string() } else { e.name.clone() };
        lines.push(EffectLine { name, text: model::changes_text(&changes) });
        temporary.extend(e.changes);
    }
    (lines, temporary)
}

/// The player's health out of its whole, as the STATS page's HP card reads it
/// ([`snapshot`]); `None` without the scripts' game. For the ITEMS page's HP card.
pub fn player_hp(forms: &mut Forms, sim: &mut SimState, script: Option<&mut ScriptState>) -> Option<(f64, f64)> {
    let rt = script.and_then(|s| s.rt.as_mut())?;
    let (_, temporary) = worn(forms, sim);
    crate::script::with_world(rt, sim, |rt| {
        let g = &mut rt.game;
        let t = gf::actor_value_temporary(g, FormId::PLAYER_REF, av::HEALTH);
        shown_and_permanent(g, &temporary, av::HEALTH).map(|(v, max)| (v, max + t))
    })
}

/// An actor value's short name, else its name.
fn short(forms: &mut Forms, code: u16) -> String {
    let t = forms.value(code);
    t.short.unwrap_or(t.name)
}

/// A list row of an actor value, its figure not read yet.
fn av_row(forms: &mut Forms, code: u16) -> Row {
    let t = forms.value(code);
    Row { name: t.name, value: Some("--".into()), mark: None, description: t.description, icon: t.icon }
}

impl Forms {
    /// An actor value's short name (AVIF ANAM), else its name.
    pub fn short_name(&mut self, code: u16) -> String {
        short(self, code)
    }

    /// The actor value of effect `index` of a magic item (its `index`th EFIT).
    fn ench_code(&mut self, item: u32, index: usize) -> Option<u16> {
        let (_, f) = fields(self.lo, self.schema, item)?;
        let s = f.subs.iter().filter(|s| &s.sig == b"EFIT").nth(index)?;
        f.decode(s)?.get("actor_value")?.as_str().and_then(gf::av_by_name)
    }
}
