//! Authored abilities and worn enchantments as active effects, on a synthetic plugin (no
//! game data): an actor's record and race SPLO, the template's "Actor Effect List",
//! enchanted armour while worn, conditions, RemoveSpell / Dispel / DispelAllSpells and
//! saving. docs/spec-notes/aid-items.md §13.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::{actor_value, set_actor_value_current, Ctx};
use fnv_game::game::{Game, GameConfig};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const RACE_GHOUL: u32 = 0x300;
const RACE_HUMAN: u32 = 0x301;
const MOD_DT: u32 = 0x850;
const MOD_RAD: u32 = 0x851;
const MOD_PER: u32 = 0x852;
const REDUCE_AGI: u32 = 0x853;
const DT_ABILITY: u32 = 0x870;
const RAD_IMMUNITY: u32 = 0x871;
const AURA: u32 = 0x872;
const GHOUL_RAD: u32 = 0x873;
const REDUCE_PER: u32 = 0x854;
const HEAD_CRIPPLED: u32 = 0x874;
const ENCH_HAT: u32 = 0x880;
const ENCH_GLASSES: u32 = 0x881;
const ENCH_METAL: u32 = 0x882;
const HAT: u32 = 0x890;
const GLASSES: u32 = 0x891;
const METAL: u32 = 0x892;
const PERK_SPELL_1: u32 = 0x875;
const PERK_SPELL_2: u32 = 0x876;
const CONCUSSED: u32 = 0x877;
const SHARP_EYES: u32 = 0x8A0;
const ROBOT: u32 = 0x400;
const ROBOT_TEMPLATE: u32 = 0x401;
/// Takes its actor effects from the robot, so from the robot's template.
const ROBOT_COPY: u32 = 0x40F;
const GHOUL: u32 = 0x402;
const ROBOT_REF: u32 = 0x500;
const GHOUL_REF: u32 = 0x501;

const P: FormId = FormId::PLAYER_REF;
const PERCEPTION: u16 = 6;
const AGILITY: u16 = 10;
const LUCK: u16 = 11;
const RAD_RESIST: u16 = 20;
const DAMAGE_THRESHOLD: u16 = 76;

const ADD_SPELL: u16 = 0x101C;
const REMOVE_SPELL: u16 = 0x101D;
const DISPEL: u16 = 0x1142;
const DISPEL_ALL: u16 = 0x1148;
const IS_SPELL_TARGET: u16 = 0x10DF;
const HAS_MAGIC_EFFECT: u16 = 0x10D6;
const GET_AV: u16 = 0x100E;
const GET_PERMANENT_AV: u16 = 0x11EF;
const GET_BASE_AV: u16 = 0x1115;
const EQUIP: u16 = 0x10EE;
const UNEQUIP: u16 = 0x10EF;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// MGEF DATA (72 bytes): flags, archetype and actor value; everything else empty.
fn mgef(fid: u32, edid: &str, flags: u32, av: i32) -> Vec<u8> {
    let data = [u32s(&[flags]), f32b(0.0), u32s(&[0]), vec![0xFF; 8], vec![0; 4], vec![0; 4], f32b(1.0), vec![0; 24], vec![0; 8], u32s(&[0]), av.to_le_bytes().to_vec()].concat();
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", data)])
}

/// EFID + EFIT (magnitude, area, duration, type Self, actor value), with conditions.
fn effect(base: u32, magnitude: u32, av: i32, conds: Vec<fnv_esm::raw::Subrecord>) -> Vec<fnv_esm::raw::Subrecord> {
    effect_for(base, magnitude, 0, av, conds)
}

/// [`effect`] with a duration in seconds (EFIT duration).
fn effect_for(base: u32, magnitude: u32, duration: u32, av: i32, conds: Vec<fnv_esm::raw::Subrecord>) -> Vec<fnv_esm::raw::Subrecord> {
    let mut v = vec![sub(b"EFID", base.to_le_bytes().to_vec()), sub(b"EFIT", [u32s(&[magnitude, 0, duration, 0]), av.to_le_bytes().to_vec()].concat())];
    v.extend(conds);
    v
}

/// A condition `GetActorValue PerceptionCondition <= 0` on the subject, as the masters'
/// `HeadCrippleEffect` has it (comparison byte 0xA0).
fn head_crippled() -> fnv_esm::raw::Subrecord {
    let mut d = vec![0xA0, 0, 0, 0];
    d.extend(0f32.to_le_bytes());
    d.extend(14u16.to_le_bytes());
    d.extend([0, 0]);
    d.extend(25u32.to_le_bytes());
    d.extend([0; 12]);
    sub(b"CTDA", d)
}

/// A condition `GetActorValue Luck >= value` on the subject.
fn luck_at_least(value: f32) -> fnv_esm::raw::Subrecord {
    let mut d = vec![0x60, 0, 0, 0];
    d.extend(value.to_le_bytes());
    d.extend(14u16.to_le_bytes());
    d.extend([0, 0]);
    d.extend(u32::from(LUCK).to_le_bytes());
    d.extend([0; 12]);
    sub(b"CTDA", d)
}

fn spel(fid: u32, edid: &str, kind: u32, effects: Vec<Vec<fnv_esm::raw::Subrecord>>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"SPIT", [u32s(&[kind, 0, 0]), vec![0; 4]].concat())];
    subs.extend(effects.into_iter().flatten());
    record(b"SPEL", 0, fid, &subs)
}

/// ENCH with ENIT (type, charge, cost, flags) and effects.
fn ench(fid: u32, edid: &str, kind: u32, effects: Vec<Vec<fnv_esm::raw::Subrecord>>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"ENIT", [u32s(&[kind, 0, 0]), vec![0; 4]].concat())];
    subs.extend(effects.into_iter().flatten());
    record(b"ENCH", 0, fid, &subs)
}

/// ARMO in one biped slot with its object effect.
fn armo(fid: u32, edid: &str, slot: u32, ench: u32) -> Vec<u8> {
    let bmdt = [u32s(&[slot]), vec![0; 4]].concat();
    let data = [u32s(&[1, 100]), f32b(1.0)].concat();
    record(b"ARMO", 0, fid, &[sub(b"EDID", z(edid)), sub(b"EITM", ench.to_le_bytes().to_vec()), sub(b"BMDT", bmdt), sub(b"DATA", data)])
}

/// ACBS with template flags (0x08 Actor Effect List [documented: xEdit]).
fn acbs(template_flags: u16) -> fnv_esm::raw::Subrecord {
    let mut d = 0u32.to_le_bytes().to_vec();
    for v in [50u16, 0, 1, 0, 0, 100] {
        d.extend(v.to_le_bytes());
    }
    d.extend(0f32.to_le_bytes());
    d.extend(0i16.to_le_bytes());
    d.extend(template_flags.to_le_bytes());
    sub(b"ACBS", d)
}

fn splo(spell: u32) -> fnv_esm::raw::Subrecord {
    sub(b"SPLO", spell.to_le_bytes().to_vec())
}

fn npc(fid: u32, edid: &str, extra: Vec<fnv_esm::raw::Subrecord>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())];
    subs.extend(extra);
    record(b"NPC_", 0, fid, &subs)
}

fn placed(fid: u32, base: u32) -> Vec<u8> {
    record(b"ACHR", 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
}

fn plugin_bytes() -> Vec<u8> {
    // MGEF flag 2 Recover, 4 Detrimental.
    let mgefs = [mgef(MOD_DT, "IncDT", 2, 76), mgef(MOD_RAD, "IncRad", 2, 20), mgef(MOD_PER, "IncPer", 2, 6), mgef(REDUCE_AGI, "RedAgi", 2 | 4, 10), mgef(REDUCE_PER, "ReducePerception", 2 | 4, 6)].concat();
    let spels = [
        spel(DT_ABILITY, "RobotDT", 4, vec![effect(MOD_DT, 15, 76, vec![])]),
        spel(RAD_IMMUNITY, "RadImmunity", 4, vec![effect(MOD_RAD, 100, 20, vec![])]),
        spel(AURA, "Aura", 0, vec![effect(MOD_PER, 1, 6, vec![])]),
        spel(GHOUL_RAD, "GhoulRad", 4, vec![effect(MOD_RAD, 50, 20, vec![])]),
        spel(HEAD_CRIPPLED, "HeadCrippleEffect", 4, vec![effect(REDUCE_PER, 4, 6, vec![head_crippled()])]),
        spel(PERK_SPELL_1, "PerkSharpEyes1", 4, vec![effect(MOD_PER, 1, 6, vec![])]),
        spel(PERK_SPELL_2, "PerkSharpEyes2", 4, vec![effect(MOD_PER, 2, 6, vec![])]),
        // As the masters' PlayerConcussed: an Actor Effect lasting ten years, its Reduced
        // Perception only while the head is crippled.
        spel(CONCUSSED, "PlayerConcussed", 0, vec![effect_for(REDUCE_PER, 4, 315_360_000, 6, vec![head_crippled()])]),
    ]
    .concat();
    // A perk of two ranks, an ability each holding the whole bonus (as Action Boy's).
    let perk_subs = vec![
        sub(b"EDID", z("SharpEyes")),
        sub(b"DATA", vec![0, 1, 2, 1, 0]),
        sub(b"PRKE", vec![1, 0, 0]),
        sub(b"DATA", PERK_SPELL_1.to_le_bytes().to_vec()),
        sub(b"PRKF", Vec::new()),
        sub(b"PRKE", vec![1, 1, 0]),
        sub(b"DATA", PERK_SPELL_2.to_le_bytes().to_vec()),
        sub(b"PRKF", Vec::new()),
    ];
    let perks = record(b"PERK", 0, SHARP_EYES, &perk_subs);
    let enchs = [
        ench(ENCH_HAT, "EnchHat", 3, vec![effect(MOD_PER, 1, 6, vec![])]),
        ench(ENCH_GLASSES, "EnchGlasses", 3, vec![effect(MOD_PER, 2, 6, vec![luck_at_least(7.0)])]),
        ench(ENCH_METAL, "EnchMetal", 3, vec![effect(REDUCE_AGI, 1, 10, vec![])]),
    ]
    .concat();
    let armos = [armo(HAT, "Hat", 1 << 0, ENCH_HAT), armo(GLASSES, "Glasses", 1 << 15, ENCH_GLASSES), armo(METAL, "Metal", 1 << 2, ENCH_METAL)].concat();
    let races = [
        record(b"RACE", 0, RACE_GHOUL, &[sub(b"EDID", z("Ghoul")), splo(GHOUL_RAD)]),
        record(b"RACE", 0, RACE_HUMAN, &[sub(b"EDID", z("Human"))]),
    ]
    .concat();
    let npcs = [
        npc(0x7, "Player", vec![acbs(0), sub(b"RNAM", RACE_HUMAN.to_le_bytes().to_vec())]),
        // The robot's own SPLO is ignored: its template supplies the actor effects.
        npc(ROBOT, "Robot", vec![acbs(0x08), sub(b"TPLT", ROBOT_TEMPLATE.to_le_bytes().to_vec()), splo(RAD_IMMUNITY)]),
        npc(ROBOT_COPY, "RobotCopy", vec![acbs(0x08), sub(b"TPLT", ROBOT.to_le_bytes().to_vec())]),
        npc(ROBOT_TEMPLATE, "RobotTemplate", vec![acbs(0), splo(DT_ABILITY), splo(RAD_IMMUNITY), splo(AURA)]),
        npc(GHOUL, "Ghoul", vec![acbs(0), sub(b"RNAM", RACE_GHOUL.to_le_bytes().to_vec())]),
    ]
    .concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs = [placed(ROBOT_REF, ROBOT), placed(GHOUL_REF, GHOUL)].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"MGEF", &mgefs),
        top(b"RACE", &races),
        top(b"NPC_", &npcs),
        top(b"SPEL", &spels),
        top(b"PERK", &perks),
        top(b"ENCH", &enchs),
        top(b"ARMO", &armos),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Abilities.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    Fixture { data, forms, refs }
}

/// The player in the test cell with Perception 5, Agility 5 and Luck 5.
fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    for (code, v) in [(PERCEPTION, 5.0), (AGILITY, 5.0), (LUCK, 5.0)] {
        set_actor_value_current(&mut g, P, code, v).unwrap();
    }
    g
}

fn call(g: &mut Game, on: FormId, op: u16, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: op, on: Some(on), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    let before = g.events.len();
    let v = g.call_function(&ctx, args);
    for e in &g.events[before..] {
        if let EventKind::CallFailed { name, error, .. } | EventKind::Unhandled { name, unit: error, .. } = &e.kind {
            panic!("0x{op:04X} {name}: {error}");
        }
    }
    match v {
        Value::Num(x) => x,
        Value::Ref(f) => f as f64,
    }
}

fn form(x: u32) -> ArgValue {
    ArgValue::Value(Value::Ref(x))
}

fn code(c: u16) -> ArgValue {
    ArgValue::Value(Value::Num(f64::from(c)))
}

fn target(g: &mut Game, on: FormId, item: u32) -> f64 {
    call(g, on, IS_SPELL_TARGET, &[form(item)])
}

fn av(g: &mut Game, on: FormId, c: u16) -> f64 {
    call(g, on, GET_AV, &[code(c)])
}

#[test]
fn records_are_read() {
    let f = fixture();
    let d = &f.data;
    assert_eq!(d.actors[&FormId(ROBOT_TEMPLATE)].spells, vec![FormId(DT_ABILITY), FormId(RAD_IMMUNITY), FormId(AURA)]);
    assert!(d.actors[&FormId(ROBOT)].template_spells);
    assert_eq!(d.race_spells[&FormId(RACE_GHOUL)], vec![FormId(GHOUL_RAD)]);
    assert_eq!(d.armor_enchantments[&FormId(HAT)], FormId(ENCH_HAT));
    assert_eq!(d.enchantment_types[&FormId(ENCH_HAT)], 3);
}

/// The template's abilities (not the record's own list, not the Actor Effect "Aura")
/// are on the robot before it is loaded, as soon as a script asks; they raise its
/// values in the temporary pool (GECK wiki ActorValue: the permanent pool only for the
/// player).
#[test]
fn authored_abilities_are_effects() {
    let f = fixture();
    let mut g = game(&f);
    let robot = FormId(ROBOT_REF);
    assert_eq!(target(&mut g, robot, DT_ABILITY), 1.0);
    assert_eq!(target(&mut g, robot, RAD_IMMUNITY), 1.0);
    assert_eq!(target(&mut g, robot, AURA), 0.0);
    assert_eq!(call(&mut g, robot, HAS_MAGIC_EFFECT, &[form(MOD_DT)]), 1.0);
    assert_eq!(av(&mut g, robot, DAMAGE_THRESHOLD), 15.0);
    assert_eq!(call(&mut g, robot, GET_PERMANENT_AV, &[code(DAMAGE_THRESHOLD)]), 0.0);
    // The race's ability.
    assert_eq!(target(&mut g, FormId(GHOUL_REF), GHOUL_RAD), 1.0);
    let base = call(&mut g, FormId(GHOUL_REF), GET_BASE_AV, &[code(RAD_RESIST)]);
    assert_eq!(av(&mut g, FormId(GHOUL_REF), RAD_RESIST), base + 50.0);
    // Given once: another frame adds nothing.
    let n = g.effects.len();
    g.tick();
    assert_eq!(g.effects.len(), n);
    assert_eq!(av(&mut g, robot, DAMAGE_THRESHOLD), 15.0);
    // The player has none.
    assert_eq!(target(&mut g, P, RAD_IMMUNITY), 0.0);
}

/// The loaded actors get theirs in the frame, without a script asking.
#[test]
fn loaded_actors_get_abilities_in_the_frame() {
    let f = fixture();
    let mut g = game(&f);
    g.tick();
    assert!(g.effects.iter().any(|e| e.target == FormId(ROBOT_REF) && e.source == FormId(DT_ABILITY)));
    assert!(g.effects.iter().any(|e| e.target == FormId(GHOUL_REF) && e.source == FormId(GHOUL_RAD)));
}

/// Abilities are on an actor only while it is loaded (capture abilities-1: Victor, Rex
/// and Lily out of the loaded area read 0), and come back when it is loaded again.
#[test]
fn abilities_only_while_loaded() {
    let f = fixture();
    let mut g = game(&f);
    let robot = FormId(ROBOT_REF);
    g.loaded_cells.remove(&FormId(CELL));
    assert_eq!(target(&mut g, robot, DT_ABILITY), 0.0);
    assert_eq!(av(&mut g, robot, DAMAGE_THRESHOLD), 0.0);
    g.loaded_cells.insert(FormId(CELL));
    g.tick();
    assert_eq!(av(&mut g, robot, DAMAGE_THRESHOLD), 15.0);
    // Leaving the loaded area ends them in the frame.
    g.loaded_cells.remove(&FormId(CELL));
    g.tick();
    assert!(!g.effects.iter().any(|e| e.target == robot));
    assert_eq!(av(&mut g, robot, DAMAGE_THRESHOLD), 0.0);
}

/// Dispel takes an ability off while the actor stays loaded; DispelAllSpells leaves the
/// others; RemoveSpell takes one away for good; AddSpell puts it back (capture
/// abilities-1, Victor).
#[test]
fn remove_spell_takes_an_authored_ability() {
    let f = fixture();
    let mut g = game(&f);
    let robot = FormId(ROBOT_REF);
    call(&mut g, robot, DISPEL, &[form(DT_ABILITY)]);
    assert_eq!(target(&mut g, robot, DT_ABILITY), 0.0);
    assert_eq!(av(&mut g, robot, DAMAGE_THRESHOLD), 0.0);
    g.tick();
    assert_eq!(target(&mut g, robot, DT_ABILITY), 0.0);
    call(&mut g, robot, DISPEL_ALL, &[]);
    assert_eq!(target(&mut g, robot, RAD_IMMUNITY), 1.0);
    // Loaded again: the dispelled ability is back.
    g.loaded_cells.remove(&FormId(CELL));
    g.tick();
    g.loaded_cells.insert(FormId(CELL));
    g.tick();
    assert_eq!(target(&mut g, robot, DT_ABILITY), 1.0);
    call(&mut g, robot, REMOVE_SPELL, &[form(DT_ABILITY)]);
    assert_eq!(target(&mut g, robot, DT_ABILITY), 0.0);
    assert_eq!(av(&mut g, robot, DAMAGE_THRESHOLD), 0.0);
    assert_eq!(target(&mut g, robot, RAD_IMMUNITY), 1.0);
    for _ in 0..3 {
        g.tick();
    }
    assert_eq!(target(&mut g, robot, DT_ABILITY), 0.0);
    call(&mut g, robot, ADD_SPELL, &[form(DT_ABILITY)]);
    assert_eq!(av(&mut g, robot, DAMAGE_THRESHOLD), 15.0);
}

/// A RemoveSpell with no reference in a script that has none (the doctors' cure in a
/// quest stage result, VDoctors / Doctors stage 20) acts on the player (o5_03, observer
/// check 42, #1110).
#[test]
fn remove_spell_with_no_reference_acts_on_the_player() {
    let f = fixture();
    let mut g = game(&f);
    call(&mut g, P, ADD_SPELL, &[form(DT_ABILITY)]);
    assert_eq!(target(&mut g, P, DT_ABILITY), 1.0);
    let ctx = Ctx { unit: "stage".into(), opcode: REMOVE_SPELL, on: None, self_ref: None, action_ref: None, quest: Some(FormId(0x999)), inst: None, in_condition: false };
    let before = g.events.len();
    g.call_function(&ctx, &[form(DT_ABILITY)]);
    assert!(!g.events[before..].iter().any(|e| matches!(e.kind, EventKind::CallFailed { .. })), "no call error");
    assert_eq!(target(&mut g, P, DT_ABILITY), 0.0);
    assert_eq!(av(&mut g, P, DAMAGE_THRESHOLD), 0.0);
    // A script with its own reference still removes the spell from that reference.
    let robot = FormId(ROBOT_REF);
    let ctx = Ctx { self_ref: Some(robot), quest: None, ..ctx };
    g.call_function(&ctx, &[form(RAD_IMMUNITY)]);
    assert_eq!(target(&mut g, robot, RAD_IMMUNITY), 0.0);
    assert_eq!(target(&mut g, robot, DT_ABILITY), 1.0);
}

/// A worn item's enchantment is active while it is equipped, in the temporary pool,
/// and ends when it is unequipped or leaves the inventory.
#[test]
fn worn_enchantments_follow_equipment() {
    let f = fixture();
    let mut g = game(&f);
    g.world.add_item(P, FormId(HAT), 1).unwrap();
    assert_eq!(target(&mut g, P, ENCH_HAT), 0.0);
    call(&mut g, P, EQUIP, &[form(HAT)]);
    assert_eq!(target(&mut g, P, ENCH_HAT), 1.0);
    assert_eq!(call(&mut g, P, HAS_MAGIC_EFFECT, &[form(MOD_PER)]), 1.0);
    assert_eq!(av(&mut g, P, PERCEPTION), 6.0);
    assert_eq!(call(&mut g, P, GET_PERMANENT_AV, &[code(PERCEPTION)]), 5.0);
    // DispelAllSpells and RemoveSpell leave a worn enchantment; Dispel takes it off
    // until the item is equipped again (capture abilities-1, the gambler hat).
    call(&mut g, P, DISPEL_ALL, &[]);
    call(&mut g, P, REMOVE_SPELL, &[form(ENCH_HAT)]);
    assert_eq!(av(&mut g, P, PERCEPTION), 6.0);
    call(&mut g, P, DISPEL, &[form(ENCH_HAT)]);
    g.tick();
    assert_eq!(target(&mut g, P, ENCH_HAT), 0.0);
    assert_eq!(av(&mut g, P, PERCEPTION), 5.0);
    call(&mut g, P, UNEQUIP, &[form(HAT)]);
    call(&mut g, P, EQUIP, &[form(HAT)]);
    assert_eq!(target(&mut g, P, ENCH_HAT), 1.0);
    assert_eq!(av(&mut g, P, PERCEPTION), 6.0);
    call(&mut g, P, UNEQUIP, &[form(HAT)]);
    assert_eq!(target(&mut g, P, ENCH_HAT), 0.0);
    assert_eq!(av(&mut g, P, PERCEPTION), 5.0);
    // Equipped outside a script (the Pip-Boy): the frame picks it up.
    g.world.equip(P, FormId(HAT)).unwrap();
    g.tick();
    assert_eq!(av(&mut g, P, PERCEPTION), 6.0);
    g.world.remove_item(P, FormId(HAT), 1).unwrap();
    g.tick();
    assert_eq!(target(&mut g, P, ENCH_HAT), 0.0);
    assert_eq!(av(&mut g, P, PERCEPTION), 5.0);
    // A harmful one lowers the value while worn.
    g.world.add_item(P, FormId(METAL), 1).unwrap();
    call(&mut g, P, EQUIP, &[form(METAL)]);
    assert_eq!(av(&mut g, P, AGILITY), 4.0);
    call(&mut g, P, UNEQUIP, &[form(METAL)]);
    assert_eq!(av(&mut g, P, AGILITY), 5.0);
}

/// An effect whose conditions fail is listed but changes nothing, and takes hold once
/// they pass (checked every second).
#[test]
fn conditions_are_checked_while_worn() {
    let f = fixture();
    let mut g = game(&f);
    g.world.add_item(P, FormId(GLASSES), 1).unwrap();
    call(&mut g, P, EQUIP, &[form(GLASSES)]);
    assert_eq!(target(&mut g, P, ENCH_GLASSES), 1.0);
    // Listed while its conditions fail (capture abilities-1, the Four Eyes glasses).
    assert_eq!(call(&mut g, P, HAS_MAGIC_EFFECT, &[form(MOD_PER)]), 1.0);
    assert_eq!(av(&mut g, P, PERCEPTION), 5.0);
    set_actor_value_current(&mut g, P, LUCK, 8.0).unwrap();
    for _ in 0..90 {
        g.tick();
    }
    assert_eq!(call(&mut g, P, HAS_MAGIC_EFFECT, &[form(MOD_PER)]), 1.0);
    assert_eq!(av(&mut g, P, PERCEPTION), 7.0);
    set_actor_value_current(&mut g, P, LUCK, 2.0).unwrap();
    for _ in 0..90 {
        g.tick();
    }
    assert_eq!(av(&mut g, P, PERCEPTION), 5.0);
    call(&mut g, P, UNEQUIP, &[form(GLASSES)]);
    assert_eq!(target(&mut g, P, ENCH_GLASSES), 0.0);
    assert_eq!(actor_value(&mut g, P, PERCEPTION).unwrap(), 5.0);
}

/// The player's race abilities follow a race chosen in the character creator.
#[test]
fn a_race_change_swaps_race_abilities() {
    let f = fixture();
    let mut g = game(&f);
    g.tick();
    assert_eq!(target(&mut g, P, GHOUL_RAD), 0.0);
    let base = av(&mut g, P, RAD_RESIST);
    let look = serde_json::json!({ "race": serde_json::to_value(FormId(RACE_GHOUL)).unwrap(), "female": false });
    g.player.look = Some(serde_json::from_value(look).unwrap());
    g.tick();
    assert_eq!(target(&mut g, P, GHOUL_RAD), 1.0);
    // A player's ability without conditions is in the permanent pool.
    assert_eq!(call(&mut g, P, GET_PERMANENT_AV, &[code(RAD_RESIST)]), base + 50.0);
    g.player.look.as_mut().unwrap().race = FormId(RACE_HUMAN);
    g.tick();
    assert_eq!(target(&mut g, P, GHOUL_RAD), 0.0);
    assert_eq!(av(&mut g, P, RAD_RESIST), base);
}

/// Saved and restored: abilities are not given twice, worn effects stay tied to their
/// item.
#[test]
fn abilities_and_worn_effects_are_saved() {
    let f = fixture();
    let mut g = game(&f);
    g.world.add_item(P, FormId(HAT), 1).unwrap();
    call(&mut g, P, EQUIP, &[form(HAT)]);
    g.tick();
    call(&mut g, FormId(ROBOT_REF), REMOVE_SPELL, &[form(DT_ABILITY)]);
    let s = g.snapshot();
    let text = serde_json::to_string(&s).unwrap();
    let back: fnv_game::game::GameSnapshot = serde_json::from_str(&text).unwrap();
    let world = World::restore(&f.forms, &f.refs, &back.sim).unwrap();
    let mut h = Game::restore(&f.data, world, g.cfg, &back).unwrap();
    let n = h.effects.len();
    h.tick();
    assert_eq!(h.effects.len(), n);
    assert_eq!(target(&mut h, FormId(ROBOT_REF), DT_ABILITY), 0.0);
    assert_eq!(target(&mut h, FormId(ROBOT_REF), RAD_IMMUNITY), 1.0);
    assert_eq!(av(&mut h, P, PERCEPTION), 6.0);
    call(&mut h, P, UNEQUIP, &[form(HAT)]);
    assert_eq!(av(&mut h, P, PERCEPTION), 5.0);
}

/// Every actor but the player has the engine's "Head Crippled" ability
/// (docs/spec-notes/crippled-limbs.md §2), though no record lists it: Perception −4 while
/// the head's condition is at 0 or below, back when it heals. The player's comes from the
/// masters' own script (`PlayerConcussed`).
#[test]
fn a_crippled_head_lowers_perception() {
    use fnv_game::functions::{add_actor_value_damage, restore_actor_value_damage};
    let f = fixture();
    let mut g = game(&f);
    let robot = FormId(ROBOT_REF);
    set_actor_value_current(&mut g, robot, PERCEPTION, 5.0).unwrap();
    g.tick();
    assert_eq!(target(&mut g, robot, HEAD_CRIPPLED), 1.0);
    assert_eq!(target(&mut g, P, HEAD_CRIPPLED), 0.0);
    assert_eq!(av(&mut g, robot, PERCEPTION), 5.0);
    add_actor_value_damage(&mut g, robot, 25, 100.0);
    for _ in 0..90 {
        g.tick();
    }
    assert_eq!(av(&mut g, robot, PERCEPTION), 1.0);
    restore_actor_value_damage(&mut g, robot, 25, 100.0);
    for _ in 0..90 {
        g.tick();
    }
    assert_eq!(av(&mut g, robot, PERCEPTION), 5.0);
}

/// A lasting spell cast while its effect's conditions fail keeps the effect, inactive:
/// it takes hold once they pass (the masters' `PlayerConcussed`, cast by `GenericScript`
/// while the head is whole; docs/spec-notes/crippled-limbs.md §2).
#[test]
fn a_lasting_spell_cast_before_its_conditions_pass_takes_hold_later() {
    use fnv_game::functions::{add_actor_value_damage, restore_actor_value_damage};
    let f = fixture();
    let mut g = game(&f);
    g.tick();
    assert_eq!(av(&mut g, P, PERCEPTION), 5.0);
    g.apply_magic_item(P, FormId(CONCUSSED), None, false);
    assert_eq!(call(&mut g, P, HAS_MAGIC_EFFECT, &[form(REDUCE_PER)]), 1.0);
    assert_eq!(av(&mut g, P, PERCEPTION), 5.0);
    add_actor_value_damage(&mut g, P, 25, 100.0);
    for _ in 0..90 {
        g.tick();
    }
    assert_eq!(av(&mut g, P, PERCEPTION), 1.0);
    restore_actor_value_damage(&mut g, P, 25, 100.0);
    for _ in 0..90 {
        g.tick();
    }
    assert_eq!(av(&mut g, P, PERCEPTION), 5.0);
}

/// A perk's ability is on the player while the perk is held: the highest rank's alone,
/// and gone with the perk (docs/spec-notes/levelup.md §9).
#[test]
fn perk_abilities_follow_the_perks() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(av(&mut g, P, PERCEPTION), 5.0);
    fnv_game::perks::gain_rank(&mut g, FormId(SHARP_EYES));
    assert_eq!(av(&mut g, P, PERCEPTION), 6.0);
    assert_eq!(target(&mut g, P, PERK_SPELL_1), 1.0);
    // Rank 2: its own ability (+2) in place of rank 1's.
    fnv_game::perks::gain_rank(&mut g, FormId(SHARP_EYES));
    assert_eq!(av(&mut g, P, PERCEPTION), 7.0);
    assert_eq!(target(&mut g, P, PERK_SPELL_1), 0.0);
    // RemovePerk: the ability goes.
    g.player.perks.remove(&FormId(SHARP_EYES));
    assert_eq!(av(&mut g, P, PERCEPTION), 5.0);
    assert_eq!(target(&mut g, P, PERK_SPELL_2), 0.0);
}

/// GetBaseSpellListSpells reads the Use Actor Effect List template's list through NPC
/// records, in record order (actors.md §2.2; observed: tmpl3, #1057).
#[test]
fn base_spell_list_follows_the_actor_effect_template() {
    use fnv_script::vm::XVal;
    let f = fixture();
    let mut g = game(&f);
    let mut q = |body: &str| g.run_query(&format!("Begin Function {{ }}
{body}
End
"), Default::default()).expect("the query runs");
    for npc in ["Robot", "RobotCopy"] {
        assert_eq!(q(&format!("SetFunctionValue (ar_Size (GetBaseSpellListSpells {npc}))")), XVal::Num(3.0), "{npc}");
        assert_eq!(q(&format!("SetFunctionValue ((GetBaseSpellListSpells {npc})[0])")), XVal::Ref(DT_ABILITY), "{npc}");
    }
    // GenericAddForm starts from the same list: the template's three, then the new one.
    q("GenericAddForm 6, RobotCopy, GhoulRad, -2");
    assert_eq!(q("SetFunctionValue (ar_Size (GetBaseSpellListSpells RobotCopy))"), XVal::Num(4.0));
    assert_eq!(q("SetFunctionValue ((GetBaseSpellListSpells RobotCopy)[0])"), XVal::Ref(DT_ABILITY));
    assert_eq!(q("SetFunctionValue ((GetBaseSpellListSpells RobotCopy)[3])"), XVal::Ref(GHOUL_RAD));
}

/// The sources of an actor's effects, in the order they stand in the list.
fn effect_sources(g: &Game, on: FormId) -> Vec<u32> {
    g.effects.iter().filter(|e| e.target == on).map(|e| e.source.0).collect()
}

fn set_spells(g: &mut Game, on: FormId, spells: &[u32]) {
    g.extra.entry(on).or_default().spells = spells.iter().copied().map(FormId).collect();
}

/// The abilities gained start in the order of the spell list, not in FormId order (#1389:
/// the search for what is missing must not reorder them).
#[test]
fn gained_abilities_apply_in_spell_list_order() {
    let f = fixture();
    let mut g = game(&f);
    set_spells(&mut g, P, &[RAD_IMMUNITY, PERK_SPELL_2, DT_ABILITY, PERK_SPELL_1]);
    g.sync_abilities(P);
    assert_eq!(effect_sources(&g, P), vec![RAD_IMMUNITY, PERK_SPELL_2, DT_ABILITY, PERK_SPELL_1]);
    let ids: Vec<u32> = g.effects.iter().filter(|e| e.target == P).map(|e| e.id).collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "applied one after another: {ids:?}");
}

/// An ability already on keeps its effect (the same id); only those without one start,
/// after it in the list; a second sync changes nothing.
#[test]
fn kept_abilities_stay_and_only_the_missing_start() {
    let f = fixture();
    let mut g = game(&f);
    set_spells(&mut g, P, &[DT_ABILITY]);
    g.sync_abilities(P);
    let kept = g.effects.iter().find(|e| e.target == P).map(|e| e.id);
    set_spells(&mut g, P, &[PERK_SPELL_1, DT_ABILITY, RAD_IMMUNITY]);
    g.sync_abilities(P);
    assert_eq!(effect_sources(&g, P), vec![DT_ABILITY, PERK_SPELL_1, RAD_IMMUNITY]);
    assert_eq!(g.effects.iter().find(|e| e.target == P).map(|e| e.id), kept);
    let before: Vec<u32> = g.effects.iter().map(|e| e.id).collect();
    g.sync_abilities(P);
    assert_eq!(g.effects.iter().map(|e| e.id).collect::<Vec<_>>(), before);
}

/// Lost: leaving the loaded area ends all of them and forgets the dispels; back in the
/// loaded area they start again in the order of the list. One that a Dispel took off
/// stays off while loaded, and one RemoveSpell took away does not come back.
#[test]
fn lost_abilities_end_and_return_in_spell_list_order() {
    let f = fixture();
    let mut g = game(&f);
    let robot = FormId(ROBOT_REF);
    g.tick();
    assert!(effect_sources(&g, robot).contains(&DT_ABILITY));
    g.loaded_cells.remove(&FormId(CELL));
    g.extra.get_mut(&robot).unwrap().dispelled.push(FormId(RAD_IMMUNITY));
    g.sync_abilities(robot);
    assert_eq!(effect_sources(&g, robot), Vec::<u32>::new());
    assert!(g.extra[&robot].dispelled.is_empty());
    g.loaded_cells.insert(FormId(CELL));
    set_spells(&mut g, robot, &[RAD_IMMUNITY, DT_ABILITY]);
    g.sync_abilities(robot);
    assert_eq!(effect_sources(&g, robot), vec![RAD_IMMUNITY, DT_ABILITY]);

    set_spells(&mut g, P, &[DT_ABILITY, RAD_IMMUNITY, GHOUL_RAD]);
    g.extra.get_mut(&P).unwrap().dispelled.push(FormId(RAD_IMMUNITY));
    g.sync_abilities(P);
    assert_eq!(effect_sources(&g, P), vec![DT_ABILITY, GHOUL_RAD]);
    call(&mut g, P, REMOVE_SPELL, &[form(DT_ABILITY)]);
    g.sync_abilities(P);
    assert_eq!(effect_sources(&g, P), vec![GHOUL_RAD]);
}

/// A list that names an ability twice (nothing in the engine adds one twice, a save could
/// list one [design: abilities.rs, aid.rs add_spell, perks.rs all check first]): what is
/// missing is read before any of them starts, so it starts twice, and once on it starts
/// no more.
#[test]
fn a_repeated_ability_starts_for_each_entry_once_missing() {
    let f = fixture();
    let mut g = game(&f);
    set_spells(&mut g, P, &[DT_ABILITY, RAD_IMMUNITY, DT_ABILITY]);
    g.sync_abilities(P);
    assert_eq!(effect_sources(&g, P), vec![DT_ABILITY, RAD_IMMUNITY, DT_ABILITY]);
    g.sync_abilities(P);
    assert_eq!(effect_sources(&g, P), vec![DT_ABILITY, RAD_IMMUNITY, DT_ABILITY]);
}

/// Only the actor's own permanent effect, not tied to worn armour, counts as the ability
/// being on: the same spell on another actor, lasting a while, or as a worn enchantment
/// leaves it missing.
#[test]
fn only_the_actors_own_ability_effect_counts() {
    let f = fixture();
    let mut g = game(&f);
    let robot = FormId(ROBOT_REF);
    set_spells(&mut g, robot, &[DT_ABILITY]);
    g.sync_abilities(robot);
    let on_robot = g.effects.iter().find(|e| e.target == robot).unwrap().clone();
    let lasting = fnv_game::game::ActiveEffect { id: 9000, target: P, permanent: false, ..on_robot.clone() };
    let worn = fnv_game::game::ActiveEffect { id: 9001, target: P, worn: Some(FormId(HAT)), ..on_robot };
    g.effects.extend([lasting, worn]);
    set_spells(&mut g, P, &[DT_ABILITY]);
    g.sync_abilities(P);
    let own = g.effects.iter().filter(|e| e.target == P && e.permanent && e.worn.is_none() && e.source == FormId(DT_ABILITY)).count();
    assert_eq!(own, 1);
}
