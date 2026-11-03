//! An actor's authored abilities and the enchantments of what it wears, as active
//! effects (docs/spec-notes/aid-items.md §13).
//!
//! - Authored abilities ([`Game::give_abilities`]): the actor effects of type Ability in
//!   the SPLO list of the actor's record (or of the template that supplies its "Actor
//!   Effect List") and of its race join the actor's spell list once, the first time
//!   the actor is processed (loaded, or asked about by a script), so RemoveSpell takes
//!   them away and they are saved with it. A race change (the character creator) swaps
//!   the race's abilities. The abilities in the list are active while the actor is
//!   loaded ([`Game::sync_abilities`]).
//! - Perks' abilities ([`Game::sync_perk_abilities`], `crate::perks`): the Ability
//!   effects of the player's perks and traits join the player's list while the perk is
//!   held, and leave it with the perk; a teammate's teammate perks give theirs to the
//!   teammate while it is one ([`Game::held_perks`]).
//! - The engine's own ability ([`ENGINE_ABILITIES`]): `HeadCrippleEffect` joins the list
//!   of every actor but the player with its authored ones (docs/spec-notes/crippled-limbs.md
//!   §2).
//! - Worn enchantments ([`Game::sync_worn`]): the object effect (ENCH of type Apparel)
//!   of each equipped armour piece is active on the loaded wearer while it is equipped,
//!   and ends when it is unequipped or leaves the inventory.
//! - Effects of abilities and worn enchantments whose conditions fail stay listed but
//!   inactive (no value change, no script) and their conditions are checked again
//!   every second ([`Game::recheck_conditions`]). So are those of a spell's effects that
//!   last (a duration above 0), whether their conditions passed when it was cast or not;
//!   an instant one that fails is dropped: the masters' `PlayerConcussed`, which
//!   `GenericScript` casts on the player while the head is whole and keeps on for ten
//!   years, takes Perception only while the head is crippled [hypothesis: as the
//!   abilities'; docs/spec-notes/crippled-limbs.md §2].

use crate::game::Game;
use fnv_sim::FormId;

/// SPEL SPIT type "Ability" [documented: schema `combat.json` SPIT].
pub const SPELL_ABILITY: i64 = 4;
/// ENCH ENIT type "Apparel" [documented: schema `combat.json`; observed: all 186 ARMO EITM of
/// FalloutNV.esm name one of this type, all 59 WEAP EITM one of type 2 Weapon].
pub const ENCHANTMENT_APPAREL: u32 = 3;
/// Seconds between checks of an ability's or a worn enchantment's conditions
/// [hypothesis: no source gives the interval].
pub const CONDITION_INTERVAL: f64 = 1.0;

/// Abilities no record lists that every actor but the player has: "Head Crippled",
/// Perception −4 while its Perception Condition is at 0 or below [observed: FalloutNV.esm
/// SPEL `HeadCrippleEffect` 0x00067572, type Ability, effect Reduced Perception 4 with
/// the condition GetActorValue PerceptionCondition <= 0; no RACE or NPC_ lists it;
/// hypothesis: the engine gives it to the actors, as the game's help text says the
/// head's crippling "applies to you as well as to your enemies"]. Not the player: the
/// masters' `GenericScript` (SCPT 0x00032DF1) casts `PlayerConcussed` on the player
/// whenever the player lacks its Concussion effect, and that spell carries the same
/// Perception −4 under the same condition, so the player's comes from the data
/// [observed] (docs/spec-notes/crippled-limbs.md §2).
pub const ENGINE_ABILITIES: [&str; 1] = ["HeadCrippleEffect"];

/// The wanted abilities that have no effect on `r` yet (permanent, not a worn
/// enchantment's), in the order given, a repeated one each time it is given. One walk over
/// the effects, crossing off what it finds, in place of a search per ability (#1389); it
/// stops once none is left to find and does not start with none wanted, so it never
/// visits more effects than those searches, each of which stopped at its first match
/// [design].
fn missing_abilities<'e>(r: FormId, wanted: impl Iterator<Item = FormId> + Clone, effects: impl IntoIterator<Item = &'e crate::game::ActiveEffect>) -> Vec<FormId> {
    let mut unseen = std::collections::BTreeSet::new();
    unseen.extend(wanted.clone());
    if unseen.is_empty() {
        return Vec::new();
    }
    for e in effects {
        if e.target == r && e.permanent && e.worn.is_none() && unseen.remove(&e.source) && unseen.is_empty() {
            return Vec::new();
        }
    }
    wanted.filter(|s| unseen.contains(s)).collect()
}

impl Game<'_> {
    /// The race whose abilities the actor has: the character creator's choice for the
    /// player, else the record's (template flag "Traits").
    pub fn ability_race(&self, r: FormId) -> Option<FormId> {
        // AgeRace's change included (`crate::appearance`).
        self.actor_race(r)
    }

    /// The abilities among a list of actor effects: type Ability only. An SPLO entry of
    /// type Actor Effect (Ruthless, the Tunnelers' DT spells, SporeCarrierRegen, auras
    /// with an area) is not on the actor [observed 2026-10-30, capture o4_00_probe:
    /// placed copies of a Tunneler, a Hulking Tunneler, a spore carrier and a Hoover Dam
    /// legionary read IsSpellTarget 0 for their Actor Effect spells and their DT, Heal Rate
    /// and Melee Damage at the base value, while the spore carrier's Ability
    /// SporeCarrierPoisonResist read IsSpellTarget 1 and Poison Resistance base 20 + 100;
    /// docs/spec-notes/area-effects.md §3].
    fn abilities_of<'s>(&'s self, spells: &'s [FormId]) -> impl Iterator<Item = FormId> + 's {
        spells.iter().copied().filter(|s| self.data.spell_types.get(s) == Some(&SPELL_ABILITY))
    }

    /// The abilities of the actor's record: the SPLO list of the record that supplies
    /// its actor effects (template flag "Actor Effect List") [documented: schema ACBS
    /// template flags; observed: GSVictorRef's base Victor sets the flag and its
    /// template VSecuritronMk1TEMPLATE lists the same three abilities].
    pub fn record_abilities(&self, r: FormId) -> Vec<FormId> {
        self.def_following(r, |d| d.template_spells).map_or_else(Vec::new, |d| self.abilities_of(&d.spells).collect())
    }

    /// The abilities of a race (RACE SPLO).
    pub fn race_abilities(&self, race: Option<FormId>) -> Vec<FormId> {
        race.and_then(|r| self.data.race_spells.get(&r)).map_or_else(Vec::new, |l| self.abilities_of(l).collect())
    }

    /// Whether an actor's abilities and worn enchantments are on it: the player always,
    /// another actor while it is enabled in a loaded cell [observed: capture abilities-1:
    /// GSVictorRef, RexREF and LilyRef out of the loaded area read IsSpellTarget 0 and no
    /// modifier for their abilities, Victor loaded 1 and DT 20; hypothesis for disabled
    /// actors].
    pub fn effects_loaded(&self, r: FormId) -> bool {
        r == FormId::PLAYER_REF || (self.present(r) && self.cell_of(r).is_some_and(|c| self.loaded_cells.contains(&c)))
    }

    /// Put an actor's authored abilities in its spell list the first time it is
    /// processed, and its race's again after a race change. Not for references that are
    /// no actor. The effects follow in [`Game::sync_abilities`].
    pub fn give_abilities(&mut self, r: FormId) {
        if r != FormId::PLAYER_REF && self.def_following(r, |d| d.template_spells).is_none() {
            return;
        }
        let race = self.ability_race(r);
        let (given, had_race) = self.extra.get(&r).map_or((false, None), |e| (e.abilities_given, e.ability_race));
        if given && had_race == race {
            return;
        }
        let own = self.record_abilities(r);
        // A new race: the old race's abilities go (unless the record has them too).
        let gone: Vec<FormId> = if given { self.race_abilities(had_race).into_iter().filter(|s| !own.contains(s)).collect() } else { Vec::new() };
        let mut add = if given { Vec::new() } else { own };
        add.extend(self.race_abilities(race));
        if !given && r != FormId::PLAYER_REF {
            add.extend(ENGINE_ABILITIES.iter().filter_map(|e| self.data.by_edid(e)).filter(|s| self.data.spell_types.get(s) == Some(&SPELL_ABILITY)));
        }
        let e = self.extra.entry(r).or_default();
        e.abilities_given = true;
        e.ability_race = race;
        e.spells.retain(|s| !gone.contains(s));
        for spell in add {
            if !e.spells.contains(&spell) {
                e.spells.push(spell);
            }
        }
        for spell in gone {
            self.dispel(r, None, |e| e.source == spell && e.worn.is_none());
        }
    }

    /// Whether an effect is an ability's (an effect of a listed spell of type Ability).
    pub fn is_ability_effect(&self, e: &crate::game::ActiveEffect) -> bool {
        e.permanent && e.worn.is_none() && self.data.spell_types.get(&e.source) == Some(&SPELL_ABILITY)
    }

    /// The abilities in an actor's spell list are active while it is loaded: each one
    /// without its effects starts (unless a Dispel took it off while loaded), and they
    /// all end when the actor is not loaded [observed: abilities-1; hypothesis: an
    /// ability added with AddSpell to an actor out of the loaded area waits for it the
    /// same way, and a dispelled one comes back when the actor is loaded again].
    pub fn sync_abilities(&mut self, r: FormId) {
        if !self.effects_loaded(r) {
            let data = self.data;
            let ability = |e: &crate::game::ActiveEffect| e.permanent && e.worn.is_none() && data.spell_types.get(&e.source) == Some(&SPELL_ABILITY);
            if self.effects.iter().any(|e| e.target == r && ability(e)) {
                self.dispel(r, None, ability);
            }
            if let Some(x) = self.extra.get_mut(&r) {
                x.dispelled.clear();
            }
            return;
        }
        let Some(x) = self.extra.get(&r) else { return };
        let wanted = x.spells.iter().copied().filter(|s| self.data.spell_types.get(s) == Some(&SPELL_ABILITY) && !x.dispelled.contains(s));
        let missing = missing_abilities(r, wanted, &self.effects);
        for spell in missing {
            self.apply_magic_item(r, spell, None, true);
        }
    }

    /// The enchanted armour an actor wears: (item, its Apparel enchantment). Reads the
    /// inventory as it is; one not yet made (an actor never loaded) wears nothing.
    pub fn worn_enchantments(&self, r: FormId) -> Vec<(FormId, FormId)> {
        let Some(inv) = self.world.peek(r).and_then(|s| s.inventory.as_ref()) else { return Vec::new() };
        let mut out: Vec<(FormId, FormId)> = inv
            .equipped()
            .into_iter()
            .filter_map(|item| {
                let ench = *self.data.armor_enchantments.get(&item)?;
                (self.data.enchantment_types.get(&ench) == Some(&ENCHANTMENT_APPAREL) && self.data.effect_items.contains_key(&ench)).then_some((item, ench))
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The worn enchantments' effects follow what the actor has equipped: an item's
    /// enchantment starts when it is worn and ends (its finish blocks run) when it is
    /// not [documented: GECK wiki ActorValue, armour's object effects alter stats "until
    /// you take the armor off"; hypothesis: they begin at once].
    ///
    /// An enchantment a Dispel took off stays off while that item stays equipped
    /// [observed: abilities-1, the gambler hat's enchantment dispelled while worn was
    /// gone until the hat was equipped again]; on an actor not loaded none is active
    /// [hypothesis: as abilities].
    pub fn sync_worn(&mut self, r: FormId) {
        let worn = self.worn_enchantments(r);
        if let Some(x) = self.extra.get_mut(&r) {
            x.worn_dispelled.retain(|i| worn.iter().any(|w| w.0 == *i));
        }
        let off = self.extra.get(&r).map(|x| x.worn_dispelled.clone()).unwrap_or_default();
        let loaded = self.effects_loaded(r);
        let wanted: Vec<(FormId, FormId)> = worn.into_iter().filter(|w| loaded && !off.contains(&w.0)).collect();
        let mut have: Vec<(FormId, FormId)> = self.effects.iter().filter(|e| e.target == r).filter_map(|e| Some((e.worn?, e.source))).collect();
        have.sort();
        have.dedup();
        for &(item, ench) in &have {
            if !wanted.contains(&(item, ench)) {
                self.dispel(r, None, |e| e.worn == Some(item) && e.source == ench);
            }
        }
        for &(item, ench) in &wanted {
            if !have.contains(&(item, ench)) {
                let mods = crate::aid::Mods { worn: Some(item), ..crate::aid::Mods::default() };
                self.apply_magic_item_with(r, ench, Some(r), true, &mods);
            }
        }
    }

    /// An actor's abilities and worn enchantments brought up to date: before a script
    /// asks about its effects or values, and every frame for the loaded actors. Inside
    /// the effects' own start (a value read while a modifier is put in place) it does
    /// nothing.
    pub fn sync_actor_effects(&mut self, r: FormId) {
        if self.syncing_effects {
            return;
        }
        self.syncing_effects = true;
        self.give_abilities(r);
        if self.wants_perk_sync(r) {
            self.sync_perk_abilities_of(r);
        }
        if r == FormId::PLAYER_REF {
            self.sync_perk_weights();
        }
        self.sync_abilities(r);
        self.sync_worn(r);
        self.syncing_effects = false;
    }

    /// Every frame: the player, the actors in the loaded cells and those that still have
    /// abilities' or worn enchantments' effects (to end them when they leave).
    pub(crate) fn sync_loaded_actor_effects(&mut self) {
        let mut actors = self.package_actors();
        actors.insert(FormId::PLAYER_REF);
        actors.extend(self.effects.iter().filter(|e| e.worn.is_some() || self.is_ability_effect(e)).map(|e| e.target));
        for a in actors {
            self.sync_actor_effects(a);
        }
    }

    /// An inactive effect whose conditions now pass takes hold (value change, archetype,
    /// ScriptEffectStart); an active one whose conditions fail lets go (its modifier
    /// leaves its pool, ScriptEffectFinish runs) and stays listed [hypothesis: the
    /// effect is suspended, not removed; aid-items.md §13].
    pub(crate) fn recheck_conditions(&mut self) {
        if self.time + 1e-9 < self.condition_check_due {
            return;
        }
        self.condition_check_due = self.time + CONDITION_INTERVAL;
        let data = self.data;
        let ids: Vec<(u32, FormId, FormId, usize, Option<FormId>, bool)> = self
            .effects
            .iter()
            .filter(|e| data.effect_items.get(&e.source).and_then(|l| l.get(e.index)).is_some_and(|i| !i.conditions.is_empty()))
            .map(|e| (e.id, e.target, e.source, e.index, e.caster, e.inactive))
            .collect();
        for (id, target, source, index, caster, inactive) in ids {
            let Some(item) = data.effect_items.get(&source).and_then(|l| l.get(index)) else { continue };
            let label = format!("{} effect {index}", data.name(source));
            let pass = self.conditions_pass(&item.conditions, Some(target), caster, &label);
            if pass && inactive {
                self.activate_effect(id);
            } else if !pass && !inactive {
                self.deactivate_effect(id);
            }
        }
    }

    pub(crate) fn activate_effect(&mut self, id: u32) {
        let Some(e) = self.effects.iter_mut().find(|e| e.id == id) else { return };
        e.inactive = false;
        self.start_change(id);
        self.effect_archetype_start(id);
        self.effect_started(id);
    }

    pub(crate) fn deactivate_effect(&mut self, id: u32) {
        self.effect_finished(id);
        self.end_change(id);
        if let Some(e) = self.effects.iter_mut().find(|e| e.id == id) {
            e.inactive = true;
        }
    }
}

#[cfg(test)]
mod missing_abilities_tests {
    use super::*;
    use crate::game::ActiveEffect;
    use std::cell::Cell;

    const R: FormId = FormId(0x500);
    const OTHER: FormId = FormId(0x501);
    const SPELLS: [FormId; 4] = [FormId(0x870), FormId(0x871), FormId(0x872), FormId(0x873)];

    fn fx(target: FormId, source: FormId, permanent: bool, worn: Option<FormId>) -> ActiveEffect {
        ActiveEffect {
            id: 0,
            target,
            source,
            effect: FormId(0x850),
            index: 0,
            magnitude: 0,
            duration: 0.0,
            elapsed: 0.0,
            permanent,
            caster: None,
            script: None,
            change: None,
            usage: 0.0,
            worn,
            inactive: false,
        }
    }

    fn ability(source: FormId) -> ActiveEffect {
        fx(R, source, true, None)
    }

    /// The logic before #1389: a search of the effects per wanted ability, with the effects
    /// it visited.
    fn searched(wanted: &[FormId], effects: &[ActiveEffect]) -> (Vec<FormId>, usize) {
        let visits = Cell::new(0);
        let out = wanted
            .iter()
            .copied()
            .filter(|s| !effects.iter().inspect(|_| visits.set(visits.get() + 1)).any(|e| e.target == R && e.source == *s && e.permanent && e.worn.is_none()))
            .collect();
        (out, visits.get())
    }

    /// The set's answer, with the effects it visited.
    fn crossed_off(wanted: &[FormId], effects: &[ActiveEffect]) -> (Vec<FormId>, usize) {
        let visits = Cell::new(0);
        let out = missing_abilities(R, wanted.iter().copied(), effects.iter().inspect(|_| visits.set(visits.get() + 1)));
        (out, visits.get())
    }

    fn assert_same(wanted: &[FormId], effects: &[ActiveEffect]) {
        let (old, old_visits) = searched(wanted, effects);
        let (new, new_visits) = crossed_off(wanted, effects);
        assert_eq!(new, old, "wanted {wanted:?} effects {effects:?}");
        assert!(new_visits <= old_visits, "visits {new_visits} above {old_visits}: wanted {wanted:?} effects {effects:?}");
    }

    /// Every wanted list of up to three (repeats, every order) against every list of up to
    /// two effects of every kind: the same abilities in the same order as the searches,
    /// with no more effects visited.
    #[test]
    fn matches_the_searches_on_every_short_list() {
        let mut kinds = Vec::new();
        for target in [R, OTHER] {
            for source in &SPELLS[..3] {
                for permanent in [true, false] {
                    for worn in [None, Some(FormId(0x890))] {
                        kinds.push(fx(target, *source, permanent, worn));
                    }
                }
            }
        }
        let mut effect_lists: Vec<Vec<ActiveEffect>> = vec![Vec::new()];
        for a in &kinds {
            effect_lists.push(vec![a.clone()]);
            for b in &kinds {
                effect_lists.push(vec![a.clone(), b.clone()]);
            }
        }
        let mut wanted_lists: Vec<Vec<FormId>> = vec![Vec::new()];
        for &a in &SPELLS {
            wanted_lists.push(vec![a]);
            for &b in &SPELLS {
                wanted_lists.push(vec![a, b]);
                for &c in &SPELLS {
                    wanted_lists.push(vec![a, b, c]);
                }
            }
        }
        for wanted in &wanted_lists {
            for effects in &effect_lists {
                assert_same(wanted, effects);
            }
        }
    }

    /// Longer lists, drawn from a fixed sequence, so that the walk ends at many positions.
    #[test]
    fn matches_the_searches_on_longer_lists() {
        let mut state = 0x2545_F491_u32;
        let mut next = move |n: usize| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 16) as usize % n
        };
        for _ in 0..5000 {
            let wanted: Vec<FormId> = (0..next(9)).map(|_| SPELLS[next(4)]).collect();
            let effects: Vec<ActiveEffect> = (0..next(17)).map(|_| fx([R, R, OTHER][next(3)], SPELLS[next(4)], next(4) > 0, (next(4) == 0).then_some(FormId(0x890)))).collect();
            assert_same(&wanted, &effects);
        }
    }

    /// Order and repeats follow the wanted list, not the set's FormId order.
    #[test]
    fn keeps_the_order_and_repeats_of_the_list() {
        let [a, b, c, d] = SPELLS;
        assert_eq!(missing_abilities(R, [d, a, c, a, b].into_iter(), &[ability(b)]), vec![d, a, c, a]);
    }

    /// The saving, in effects visited: a search per ability walks the list once per
    /// ability, the set once.
    #[test]
    fn one_walk_in_place_of_a_search_per_ability() {
        let wanted = SPELLS;
        let mut effects: Vec<ActiveEffect> = (0..96).map(|_| fx(OTHER, SPELLS[0], true, None)).collect();
        // All four missing: a full walk per ability against one full walk.
        assert_eq!(searched(&wanted, &effects), (wanted.to_vec(), 4 * 96));
        assert_eq!(crossed_off(&wanted, &effects), (wanted.to_vec(), 96));
        // All four on, at the end: the searches stop at positions 97 to 100, the walk at
        // the last of them.
        effects.extend(wanted.iter().map(|s| ability(*s)));
        assert_eq!(searched(&wanted, &effects), (Vec::new(), 97 + 98 + 99 + 100));
        assert_eq!(crossed_off(&wanted, &effects), (Vec::new(), 100));
    }

    /// One ability on early in the list and none wanted: as cheap as the search, no walk
    /// over the rest.
    #[test]
    fn stops_early_and_does_not_start_with_nothing_wanted() {
        let [a, ..] = SPELLS;
        let mut effects = vec![ability(a)];
        effects.extend((0..50).map(|_| fx(OTHER, a, true, None)));
        assert_eq!(crossed_off(&[a], &effects), (Vec::new(), 1));
        assert_eq!(crossed_off(&[], &effects), (Vec::new(), 0));
    }
}
