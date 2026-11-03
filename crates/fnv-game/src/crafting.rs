//! Crafting: the recipes (RCPE) and their categories (RCCT), the recipe menu scripts open
//! (`ShowRecipeMenu`, alias `Craft`) and what crafting takes and gives
//! (docs/spec-notes/crafting.md).
//!
//! A recipe [documented: GECK wiki Recipe Entry, owner export]: a name; a category (where
//! it is made: Workbench, Campfire, Reloading Bench, …) and a subcategory (Aid, Ammo,
//! Chems, …, for sorting); a required skill and level ("NONE" for none); visibility
//! conditions (a recipe whose conditions fail is not listed at all); ingredients and
//! outputs, each an item and a quantity. "Recipes will only become usable once the
//! player has all the listed ingredients."
//!
//! The record's layout [documented: xEdit wbDefinitionsFNV RCPE: `EDID`, `FULL`, `CTDA`s,
//! `DATA` (skill as an actor value, i32, −1 none; level u32; category FormID; subcategory
//! FormID), then `RCIL` (an ingredient) and `RCOD` (an output), each followed by its
//! `RCQY` quantity, u32; RCCT: `EDID`, `FULL`, `DATA` flags u8, bit 0 Subcategory;
//! observed in FalloutNV.esm: 105 RCPE, every DATA 16 bytes, 263 RCIL + 144 RCOD = 407
//! RCQY; 10 RCCT, every DATA one byte].
//!
//! `ShowRecipeMenu category` takes a category or a subcategory [documented: GECK wiki
//! ShowRecipeMenu, its example `EDE1REF.Craft AmmoSubRecipes`]: the menu lists the
//! recipes whose category or subcategory it is. The menu is MenuMode 1077 [documented:
//! GECK wiki GetMenuTargetRef: "1077 The actor whose skills are used for crafting"].

use crate::cond::Condition;
use fnv_sim::FormId;

/// MenuMode of the recipe menu [documented: GECK wiki GetMenuTargetRef].
pub const RECIPE_MENU: u16 = 1077;

/// A recipe (RCPE).
#[derive(Debug, Clone, PartialEq)]
pub struct RecipeDef {
    pub form: FormId,
    pub edid: Option<String>,
    pub name: String,
    /// Visibility conditions, run on the player.
    pub conditions: Vec<Condition>,
    /// The skill required (an actor value code) and its level; `None` for no skill.
    pub skill: Option<(u16, u32)>,
    pub category: Option<FormId>,
    pub subcategory: Option<FormId>,
    pub ingredients: Vec<(FormId, u32)>,
    pub outputs: Vec<(FormId, u32)>,
}

impl Default for RecipeDef {
    fn default() -> Self {
        RecipeDef { form: FormId(0), edid: None, name: String::new(), conditions: Vec::new(), skill: None, category: None, subcategory: None, ingredients: Vec::new(), outputs: Vec::new() }
    }
}

/// A recipe category (RCCT).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecipeCategoryDef {
    pub name: String,
    pub subcategory: bool,
}

impl RecipeDef {
    /// Read from the record's subrecords; `map` turns a local FormID into a load-order
    /// one, `cond` reads a CTDA. Ingredient and output quantities pair with the RCIL or
    /// RCOD before them.
    pub fn from_raw(form: FormId, subs: &[fnv_esm::Subrecord], map: &dyn Fn(u32) -> Option<u32>, cond: &mut dyn FnMut(&fnv_esm::Subrecord) -> Option<Condition>) -> RecipeDef {
        let mut r = RecipeDef { form, ..Default::default() };
        let u32_at = |b: &[u8], o: usize| b.get(o..o + 4).and_then(|x| <[u8; 4]>::try_from(x).ok()).map(u32::from_le_bytes);
        let fid = |v: Option<u32>| v.filter(|&x| x != 0).and_then(map).map(FormId);
        // The list the next RCQY belongs to.
        let mut last: Option<bool> = None;
        for s in subs {
            match &s.sig {
                b"EDID" => r.edid = Some(zstring(&s.data)),
                b"FULL" => r.name = zstring(&s.data),
                b"CTDA" => r.conditions.extend(cond(s)),
                b"DATA" => {
                    let skill = u32_at(&s.data, 0).map(|v| v as i32);
                    let level = u32_at(&s.data, 4).unwrap_or(0);
                    r.skill = skill.filter(|&k| k >= 0).map(|k| (k as u16, level));
                    r.category = fid(u32_at(&s.data, 8));
                    r.subcategory = fid(u32_at(&s.data, 12));
                }
                b"RCIL" | b"RCOD" => {
                    let input = &s.sig == b"RCIL";
                    if let Some(item) = fid(u32_at(&s.data, 0)) {
                        if input {
                            r.ingredients.push((item, 1));
                        } else {
                            r.outputs.push((item, 1));
                        }
                        last = Some(input);
                    } else {
                        last = None;
                    }
                }
                b"RCQY" => {
                    let n = u32_at(&s.data, 0).unwrap_or(1);
                    let list = match last.take() {
                        Some(true) => &mut r.ingredients,
                        Some(false) => &mut r.outputs,
                        None => continue,
                    };
                    if let Some(e) = list.last_mut() {
                        e.1 = n;
                    }
                }
                _ => {}
            }
        }
        r
    }

    /// Whether `category` (a category or a subcategory) lists this recipe.
    pub fn listed_under(&self, category: FormId) -> bool {
        self.category == Some(category) || self.subcategory == Some(category)
    }

    /// The total quantity made of the first output (what the menu shows after the name).
    pub fn made(&self) -> u32 {
        self.outputs.first().map_or(0, |o| o.1)
    }
}

impl RecipeCategoryDef {
    pub fn from_raw(subs: &[fnv_esm::Subrecord]) -> RecipeCategoryDef {
        let mut c = RecipeCategoryDef::default();
        for s in subs {
            match &s.sig {
                b"FULL" => c.name = zstring(&s.data),
                b"DATA" => c.subcategory = s.data.first().is_some_and(|f| f & 1 != 0),
                _ => {}
            }
        }
        c
    }
}

fn zstring(b: &[u8]) -> String {
    // In this run's code page (fnv_util::text, #1439).
    fnv_util::text::decode_z(b).trim().to_string()
}

/// Why a recipe cannot be made now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    /// The skill is below the level: (skill, have, need).
    Skill(u16, u32, u32),
    /// Too few of an ingredient: (item, have, need).
    Ingredient(FormId, u32, u32),
}

/// What making a recipe `times` times needs that the player lacks: the skill (`skill`
/// is the player's value of an actor value) and each ingredient (`have` counts the
/// player's items). Empty: it can be made.
pub fn missing(r: &RecipeDef, times: u32, skill: impl Fn(u16) -> f64, have: impl Fn(FormId) -> i64) -> Vec<Missing> {
    let mut out = Vec::new();
    if let Some((av, level)) = r.skill {
        let v = skill(av).max(0.0).floor() as u32;
        if v < level {
            out.push(Missing::Skill(av, v, level));
        }
    }
    for &(item, n) in &r.ingredients {
        let h = have(item).max(0) as u32;
        let need = n.saturating_mul(times.max(1));
        if h < need {
            out.push(Missing::Ingredient(item, h, need));
        }
    }
    out
}

/// How many times a recipe can be made with what the player has (the skill aside).
pub fn times_possible(r: &RecipeDef, have: impl Fn(FormId) -> i64) -> u32 {
    r.ingredients.iter().map(|&(item, n)| (have(item).max(0) as u32).checked_div(n).unwrap_or(u32::MAX)).min().unwrap_or(u32::MAX)
}

/// The skills' names by actor value code [documented: GECK wiki Actor Value Codes; the
/// AVIF records' names in FalloutNV.esm].
pub fn skill_name(av: u16) -> &'static str {
    use crate::actor_values::*;
    match av {
        BARTER => "Barter",
        BIG_GUNS => "Big Guns",
        ENERGY_WEAPONS => "Energy Weapons",
        EXPLOSIVES => "Explosives",
        LOCKPICK => "Lockpick",
        MEDICINE => "Medicine",
        MELEE_WEAPONS => "Melee Weapons",
        REPAIR => "Repair",
        SCIENCE => "Science",
        GUNS => "Guns",
        SNEAK => "Sneak",
        SPEECH => "Speech",
        SURVIVAL => "Survival",
        UNARMED => "Unarmed",
        _ => "Skill",
    }
}

/// `station.ShowRecipeMenu category`: the recipe menu, for the engine to draw; it is open
/// for scripts ([`RECIPE_MENU`]) until the engine closes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecipeRequest {
    /// The reference the menu was opened on (the workbench, the campfire).
    pub station: Option<FormId>,
    pub category: FormId,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(sig: &[u8; 4], data: Vec<u8>) -> fnv_esm::Subrecord {
        fnv_esm::Subrecord { sig: *sig, data, extended: false }
    }

    fn word(v: u32) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }

    #[test]
    fn a_recipe_reads_its_data_and_pairs_quantities() {
        let mut data = word(40); // Science
        data.extend(word(45));
        data.extend(word(0x100));
        data.extend(word(0x200));
        let subs = vec![
            sub(b"EDID", b"RecipeX\0".to_vec()),
            sub(b"FULL", b"Conversion, X to Y (25)\0".to_vec()),
            sub(b"DATA", data),
            sub(b"RCIL", word(0x10)),
            sub(b"RCQY", word(5)),
            sub(b"RCIL", word(0x11)),
            sub(b"RCQY", word(1)),
            sub(b"RCOD", word(0x20)),
            sub(b"RCQY", word(25)),
        ];
        let r = RecipeDef::from_raw(FormId(7), &subs, &|l| Some(l), &mut |_| None);
        assert_eq!(r.name, "Conversion, X to Y (25)");
        assert_eq!(r.skill, Some((40, 45)));
        assert_eq!((r.category, r.subcategory), (Some(FormId(0x100)), Some(FormId(0x200))));
        assert_eq!(r.ingredients, vec![(FormId(0x10), 5), (FormId(0x11), 1)]);
        assert_eq!(r.outputs, vec![(FormId(0x20), 25)]);
        assert!(r.listed_under(FormId(0x100)) && r.listed_under(FormId(0x200)) && !r.listed_under(FormId(0x300)));
        // No skill: −1.
        let mut none = word(u32::MAX);
        none.extend([0u8; 12]);
        assert_eq!(RecipeDef::from_raw(FormId(8), &[sub(b"DATA", none)], &|l| Some(l), &mut |_| None).skill, None);
    }

    #[test]
    fn what_is_missing_and_how_often_it_can_be_made() {
        let r = RecipeDef { skill: Some((40, 45)), ingredients: vec![(FormId(1), 5), (FormId(2), 1)], outputs: vec![(FormId(3), 25)], ..Default::default() };
        let have = |n1: i64, n2: i64| move |i: FormId| if i == FormId(1) { n1 } else if i == FormId(2) { n2 } else { 0 };
        assert_eq!(missing(&r, 1, |_| 17.0, have(0, 1)), vec![Missing::Skill(40, 17, 45), Missing::Ingredient(FormId(1), 0, 5)]);
        assert!(missing(&r, 1, |_| 45.0, have(5, 1)).is_empty());
        assert_eq!(missing(&r, 2, |_| 50.0, have(9, 3)), vec![Missing::Ingredient(FormId(1), 9, 10)]);
        assert_eq!(times_possible(&r, have(12, 3)), 2);
        assert_eq!(times_possible(&r, have(12, 0)), 0);
        assert_eq!(r.made(), 25);
    }

    #[test]
    fn a_category_knows_whether_it_is_a_subcategory() {
        assert_eq!(RecipeCategoryDef::from_raw(&[sub(b"FULL", b"Workbench\0".to_vec()), sub(b"DATA", vec![0])]), RecipeCategoryDef { name: "Workbench".into(), subcategory: false });
        assert!(RecipeCategoryDef::from_raw(&[sub(b"DATA", vec![1])]).subcategory);
    }
}
