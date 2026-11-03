//! AgeRace and SexChange on the player and on NPCs, SetLevel on an actor, and SetPCCanUsePowerArmor gating power armour on the player, on a
//! synthetic plugin (no game data). docs/spec-notes/appearance-changes.md.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::world::SimError;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const HUMAN: u32 = 0x300;
const HUMAN_OLD: u32 = 0x301;
const HUMAN_CHILD: u32 = 0x302;
const GHOUL: u32 = 0x303;
const JOE: u32 = 0x400;
const JOE_REF: u32 = 0x500;
const GHOUL_NPC: u32 = 0x401;
const GHOUL_REF: u32 = 0x501;
const KNIFE: u32 = 0x600;
const SWORD: u32 = 0x601;
const BLADES: u32 = 0x610;
const POWER_ARMOR: u32 = 0x620;
const LEATHER: u32 = 0x621;

const P: FormId = FormId::PLAYER_REF;

const AGE_RACE: u16 = 0x11E4;
const SEX_CHANGE: u16 = 0x11E7;
const GET_IS_RACE: u16 = 0x1045;
const IS_CHILD: u16 = 0x116D;
const SET_LEVEL: u16 = 0x1133;
const GET_LEVEL: u16 = 0x1050;
const GET_ITEM_COUNT: u16 = 0x102F;
const SET_PC_CAN_USE_POWER_ARMOR: u16 = 0x1229;

fn race(fid: u32, edid: &str, older: Option<u32>, younger: Option<u32>, child: bool) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid))];
    // DATA: seven skill boosts, two unused bytes, heights and weights, flags (4 Child).
    let data = [[0xFF, 0].repeat(7), vec![0; 2], [1f32, 1.0, 1.0, 1.0].iter().flat_map(|f| f.to_le_bytes()).collect(), u32s(&[if child { 4 } else { 0 }])].concat();
    subs.push(sub(b"DATA", data));
    if let Some(o) = older {
        subs.push(sub(b"ONAM", o.to_le_bytes().to_vec()));
    }
    if let Some(y) = younger {
        subs.push(sub(b"YNAM", y.to_le_bytes().to_vec()));
    }
    record(b"RACE", 0, fid, &subs)
}

fn acbs(female: bool) -> fnv_esm::raw::Subrecord {
    let mut d = u32::from(female).to_le_bytes().to_vec();
    for v in [50u16, 0, 1, 0, 0, 100] {
        d.extend(v.to_le_bytes());
    }
    d.extend(0f32.to_le_bytes());
    d.extend(0i16.to_le_bytes());
    d.extend(0u16.to_le_bytes());
    sub(b"ACBS", d)
}

fn npc(fid: u32, edid: &str, race: u32, extra: Vec<fnv_esm::raw::Subrecord>) -> Vec<u8> {
    let mut subs = vec![sub(b"EDID", z(edid)), acbs(false), sub(b"RNAM", race.to_le_bytes().to_vec()), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())];
    subs.extend(extra);
    record(b"NPC_", 0, fid, &subs)
}

fn placed(fid: u32, base: u32) -> Vec<u8> {
    record(b"ACHR", 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
}

fn misc(fid: u32, edid: &str) -> Vec<u8> {
    record(b"MISC", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", [1i32.to_le_bytes(), 1f32.to_le_bytes()].concat())])
}

/// ARMO on the upper body with BMDT general flags.
fn armo(fid: u32, edid: &str, general: u8) -> Vec<u8> {
    let bmdt = [u32s(&[1 << 2]), vec![general, 0, 0, 0]].concat();
    let data = [u32s(&[1, 100]), 1f32.to_le_bytes().to_vec()].concat();
    record(b"ARMO", 0, fid, &[sub(b"EDID", z(edid)), sub(b"BMDT", bmdt), sub(b"DATA", data)])
}

fn plugin_bytes() -> Vec<u8> {
    let races = [
        race(HUMAN, "Human", Some(HUMAN_OLD), Some(HUMAN_CHILD), false),
        race(HUMAN_OLD, "HumanOld", None, Some(HUMAN), false),
        race(HUMAN_CHILD, "HumanChild", Some(HUMAN), None, true),
        race(GHOUL, "Ghoul", None, None, false),
    ]
    .concat();
    // A leveled list: the knife from level 1, the sword from level 10 (the highest entry
    // at or below the level).
    let lvli = record(
        b"LVLI",
        0,
        BLADES,
        &[
            sub(b"EDID", z("Blades")),
            sub(b"LVLD", vec![0]),
            sub(b"LVLO", [1u16.to_le_bytes().to_vec(), vec![0; 2], KNIFE.to_le_bytes().to_vec(), 1i16.to_le_bytes().to_vec(), vec![0; 2]].concat()),
            sub(b"LVLO", [10u16.to_le_bytes().to_vec(), vec![0; 2], SWORD.to_le_bytes().to_vec(), 1i16.to_le_bytes().to_vec(), vec![0; 2]].concat()),
        ],
    );
    let npcs = [
        npc(0x7, "Player", HUMAN, vec![]),
        npc(JOE, "Joe", HUMAN, vec![sub(b"CNTO", [BLADES.to_le_bytes(), 1i32.to_le_bytes()].concat())]),
        npc(GHOUL_NPC, "Gus", GHOUL, vec![]),
    ]
    .concat();
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let refs = [placed(JOE_REF, JOE), placed(GHOUL_REF, GHOUL_NPC)].concat();
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs));
    [
        tes4(),
        top(b"RACE", &races),
        top(b"MISC", &[misc(KNIFE, "Knife"), misc(SWORD, "Sword")].concat()),
        top(b"ARMO", &[armo(POWER_ARMOR, "T45", 0x20 | 0x80), armo(LEATHER, "Leather", 0x08)].concat()),
        top(b"LVLI", &lvli),
        top(b"NPC_", &npcs),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Appearance.esm".into()).unwrap()];
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

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    g
}

fn call(g: &mut Game, on: Option<FormId>, op: u16, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: op, on, self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
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

fn num(x: f64) -> ArgValue {
    ArgValue::Value(Value::Num(x))
}

fn is_race(g: &mut Game, on: FormId, race: u32) -> bool {
    call(g, Some(on), GET_IS_RACE, &[form(race)]) == 1.0
}

#[test]
fn race_links_are_read() {
    let f = fixture();
    assert_eq!(f.data.race_older.get(&FormId(HUMAN)), Some(&FormId(HUMAN_OLD)));
    assert_eq!(f.data.race_younger.get(&FormId(HUMAN)), Some(&FormId(HUMAN_CHILD)));
    assert!(!f.data.race_older.contains_key(&FormId(GHOUL)));
}

/// AgeRace walks the links and stops at the ends; younger then older comes back (the
/// opening's VCG00 stage 5); a race without links stays; IsChild follows the new race;
/// each real change is told to the engine.
#[test]
fn age_race_follows_the_links_and_clamps() {
    let f = fixture();
    let mut g = game(&f);
    let joe = FormId(JOE_REF);
    call(&mut g, Some(joe), AGE_RACE, &[num(1.0)]);
    assert!(is_race(&mut g, joe, HUMAN_OLD));
    // Past the oldest: stays old.
    call(&mut g, Some(joe), AGE_RACE, &[num(3.0)]);
    assert!(is_race(&mut g, joe, HUMAN_OLD));
    call(&mut g, Some(joe), AGE_RACE, &[num(-2.0)]);
    assert!(is_race(&mut g, joe, HUMAN_CHILD));
    assert_eq!(call(&mut g, Some(joe), IS_CHILD, &[]), 1.0);
    assert_eq!(g.take_appearance_changes().len(), 2, "the step past the end changed nothing");
    // The player: -1 then 1 is where it started; two changes for the engine.
    call(&mut g, Some(P), AGE_RACE, &[num(-1.0)]);
    assert!(is_race(&mut g, P, HUMAN_CHILD));
    call(&mut g, Some(P), AGE_RACE, &[num(1.0)]);
    assert!(is_race(&mut g, P, HUMAN));
    assert_eq!(g.take_appearance_changes().iter().filter(|c| c.actor == P).count(), 2);
    // No links: nothing.
    call(&mut g, Some(FormId(GHOUL_REF)), AGE_RACE, &[num(1.0)]);
    assert!(is_race(&mut g, FormId(GHOUL_REF), GHOUL));
    assert!(g.take_appearance_changes().is_empty());
    // Kept on the reference.
    assert_eq!(g.extra[&joe].race, Some(FormId(HUMAN_CHILD)));
}

/// SexChange on an NPC: a given sex, or the other one; GetIsSex reads it; reset counts
/// on the player only, and only for a real change.
#[test]
fn sex_change_works_on_npcs_and_the_player() {
    let f = fixture();
    let mut g = game(&f);
    let joe = FormId(JOE_REF);
    assert!(!g.actor_female(joe));
    // No sex given: the other one.
    call(&mut g, Some(joe), SEX_CHANGE, &[]);
    assert!(g.actor_female(joe));
    assert_eq!(g.extra[&joe].female, Some(true));
    // Female again: no change, nothing for the engine.
    g.take_appearance_changes();
    call(&mut g, Some(joe), SEX_CHANGE, &[num(1.0), num(1.0)]);
    assert!(g.take_appearance_changes().is_empty());
    call(&mut g, Some(joe), SEX_CHANGE, &[num(0.0), num(1.0)]);
    let c = g.take_appearance_changes();
    assert!(!g.actor_female(joe));
    assert!(c.len() == 1 && !c[0].reset, "reset is the player's only");
    // The player (no calling reference), with reset.
    call(&mut g, None, SEX_CHANGE, &[num(1.0), num(1.0)]);
    assert!(g.actor_female(P));
    let c = g.take_appearance_changes();
    assert!(c.len() == 1 && c[0].actor == P && c[0].reset);
}

/// SetLevel on an NPC changes its level only: what it carries stays, leveled items
/// included (capture ac1); on the player only the number changes.
#[test]
fn set_level_keeps_an_actors_inventory() {
    let f = fixture();
    let mut g = game(&f);
    let joe = FormId(JOE_REF);
    let count = |g: &mut Game, item: u32| call(g, Some(joe), GET_ITEM_COUNT, &[form(item)]);
    assert_eq!((count(&mut g, KNIFE), count(&mut g, SWORD)), (1.0, 0.0));
    call(&mut g, Some(joe), SET_LEVEL, &[num(12.0), num(0.0)]);
    assert_eq!(call(&mut g, Some(joe), GET_LEVEL, &[]), 12.0);
    assert_eq!((count(&mut g, KNIFE), count(&mut g, SWORD)), (1.0, 0.0));
    // With the level-to-player flag: player level 1 + 2, raised to the minimum 5.
    call(&mut g, Some(joe), SET_LEVEL, &[num(2.0), num(1.0), num(5.0), num(20.0)]);
    assert_eq!(call(&mut g, Some(joe), GET_LEVEL, &[]), 5.0);
    // The player: the level, nothing rolled.
    call(&mut g, Some(P), SET_LEVEL, &[num(6.0), num(0.0)]);
    assert_eq!(call(&mut g, Some(P), GET_LEVEL, &[]), 6.0);
}

/// Without the training the player cannot wear power armour (other armour and other
/// actors are not held back); SetPCCanUsePowerArmor 1 allows it, and the flag reaches a
/// world swapped in after a tick.
#[test]
fn power_armor_needs_the_training() {
    let f = fixture();
    let mut g = game(&f);
    for item in [POWER_ARMOR, LEATHER] {
        g.world.add_item(P, FormId(item), 1).unwrap();
        g.world.add_item(FormId(JOE_REF), FormId(item), 1).unwrap();
    }
    assert!(matches!(g.world.equip(P, FormId(POWER_ARMOR)), Err(SimError::NoPowerArmorTraining(_))));
    assert!(g.world.equip(P, FormId(LEATHER)).is_ok());
    assert!(g.world.equip(FormId(JOE_REF), FormId(POWER_ARMOR)).is_ok());
    call(&mut g, None, SET_PC_CAN_USE_POWER_ARMOR, &[num(1.0)]);
    assert!(g.player.power_armor);
    assert!(g.world.equip(P, FormId(POWER_ARMOR)).is_ok());
    // Off again: kept off by the tick too.
    call(&mut g, None, SET_PC_CAN_USE_POWER_ARMOR, &[num(0.0)]);
    g.world.set_power_armor_training(true);
    g.tick();
    assert!(!g.world.power_armor_training());
    // The refusal's message is the masters' (none here).
    assert_eq!(fnv_game::functions::refuse_power_armor(&mut g), None);
}

/// MatchRace (Fallout 3's Vault 101 exit, TTW): the caller takes the source's race now,
/// told to the engine; the same race again changes nothing. GetPCIsRace reads the
/// player's race.
#[test]
fn match_race_copies_the_race_the_source_has_now() {
    const MATCH_RACE: u16 = 0x11E5;
    const GET_PC_IS_RACE: u16 = 0x1082;
    let f = fixture();
    let mut g = game(&f);
    let joe = FormId(JOE_REF);
    assert_eq!(call(&mut g, None, GET_PC_IS_RACE, &[form(HUMAN)]), 1.0);
    call(&mut g, Some(joe), AGE_RACE, &[num(1.0)]);
    g.take_appearance_changes();
    call(&mut g, Some(P), MATCH_RACE, &[form(JOE_REF)]);
    assert!(is_race(&mut g, P, HUMAN_OLD));
    assert_eq!(call(&mut g, None, GET_PC_IS_RACE, &[form(HUMAN_OLD)]), 1.0);
    assert_eq!(call(&mut g, None, GET_PC_IS_RACE, &[form(HUMAN)]), 0.0);
    assert_eq!(g.take_appearance_changes().len(), 1);
    call(&mut g, Some(P), MATCH_RACE, &[form(JOE_REF)]);
    assert!(g.take_appearance_changes().is_empty());
    // On an NPC: the ghoul takes Joe's.
    call(&mut g, Some(FormId(GHOUL_REF)), MATCH_RACE, &[form(JOE_REF)]);
    assert!(is_race(&mut g, FormId(GHOUL_REF), HUMAN_OLD));
}
