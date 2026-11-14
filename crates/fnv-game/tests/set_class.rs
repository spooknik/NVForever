//! SetClass (an actor's base takes another class, read back by GetIsClass and GetClass)
//! and JIP's auxiliary variables with no owner (a getter gives the unset value instead of
//! stopping the statement) on a synthetic plugin.

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CLASS_A: u32 = 0x900;
const CLASS_B: u32 = 0x901;
const NPC: u32 = 0x902;
const MISC: u32 = 0x903;

fn plugin_bytes() -> Vec<u8> {
    let npc_data = [50i32.to_le_bytes().to_vec(), vec![5; 7]].concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"DATA", npc_data.clone())]),
        record(b"NPC_", 0, NPC, &[sub(b"EDID", z("TestNpc")), sub(b"CNAM", CLASS_A.to_le_bytes().to_vec()), sub(b"DATA", npc_data)]),
    ]
    .concat();
    let classes = [record(b"CLAS", 0, CLASS_A, &[sub(b"EDID", z("ClassA"))]), record(b"CLAS", 0, CLASS_B, &[sub(b"EDID", z("ClassB"))])].concat();
    let misc = record(b"MISC", 0, MISC, &[sub(b"EDID", z("TestMisc"))]);
    [tes4(), top(b"CLAS", &classes), top(b"NPC_", &npcs), top(b"MISC", &misc)].concat()
}

fn with_game(f: impl FnOnce(&mut Game)) {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Classes.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let table = FunctionTable::load(&p).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    let world = World::new(&forms, &refs, SimConfig::default());
    let mut g = Game::new(&data, world, GameConfig::default());
    f(&mut g);
}

fn q(g: &mut Game, body: &str) -> f64 {
    match g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), Default::default()).unwrap_or_else(|e| panic!("{body}: {e}")) {
        XVal::Num(x) => x,
        XVal::Ref(r) => f64::from(r),
        other => panic!("{body}: {other:?}"),
    }
}

/// [documented: SetClass, "Changes the class of the actor to the parameter"]
#[test]
fn set_class_changes_what_the_class_getters_read() {
    with_game(|g| {
        assert_eq!(q(g, "SetFunctionValue (TestNpc.GetIsClass ClassA)"), 1.0);
        q(g, "TestNpc.SetClass ClassB");
        assert_eq!(q(g, "SetFunctionValue (TestNpc.GetIsClass ClassB)"), 1.0);
        assert_eq!(q(g, "SetFunctionValue (TestNpc.GetIsClass ClassA)"), 0.0);
        assert_eq!(q(g, "ref r\nlet r := GetClass TestNpc\nSetFunctionValue r"), f64::from(CLASS_B));
        // Not a class: nothing changes.
        q(g, "TestNpc.SetClass TestMisc");
        assert_eq!(q(g, "SetFunctionValue (TestNpc.GetIsClass ClassB)"), 1.0);
    });
}

/// A getter with an empty ref variable for its owner, on a script with no calling
/// reference, gives the unset value, and the `let` takes it [documented:
/// AuxiliaryVariableGetFloat, "return 0 if the variable is not defined"; hypothesis: no
/// owner alike]; a setter gives 0 and keeps nothing.
#[test]
fn aux_variables_without_an_owner_read_as_unset() {
    with_game(|g| {
        assert_eq!(q(g, "float f\nref r\nlet f := 5\nlet f := AuxVarGetFlt \"*x\" 0 r\nSetFunctionValue f"), 0.0);
        assert_eq!(q(g, "ref r\nSetFunctionValue (AuxVarSetFlt \"*x\" 3 0 r)"), 0.0);
        assert_eq!(q(g, "ref r\nSetFunctionValue (AuxiliaryVariableGetSize \"*x\" r)"), 0.0);
        assert_eq!(q(g, "float f\nref r\nlet f := 5\nlet f := RefMapArrayGetFloat \"*m\" r\nSetFunctionValue f"), 0.0);
    });
}

/// SetFormDescription with a specifier and no value after it leaves the text as it was
/// and the script goes on; with the value the specifier is filled [observed: #2430 probe
/// 2026-11-14, Toughness: "test %.0f left" alone unchanged, with 12.6 "test 13 left"].
#[test]
fn set_form_description_needs_its_values() {
    with_game(|g| {
        let desc = |g: &mut Game| match g.run_query("Begin Function { }\nSetFunctionValue (GetFormDescription TestMisc)\nEnd\n", Default::default()).unwrap() {
            XVal::Str(s) => s,
            other => panic!("{other:?}"),
        };
        q(g, "SetFormDescription TestMisc \"nvf reset\"");
        assert_eq!(desc(g), "nvf reset");
        assert_eq!(q(g, "SetFormDescription TestMisc \"test %.0f left\"\nSetFunctionValue 7"), 7.0, "the script goes on");
        assert_eq!(desc(g), "nvf reset");
        q(g, "float f\nlet f := 12.6\nSetFormDescription TestMisc \"test %.0f left\" f");
        assert_eq!(desc(g), "test 13 left");
    });
}

/// GPCDEx answers as GetPlayerControlsDisabledAltEx [observed: #2430 probe 2026-11-14].
#[test]
fn gpcdex_reads_the_disabled_controls() {
    with_game(|g| {
        assert_eq!(q(g, "SetFunctionValue (GPCDEx 2 4)"), 0.0);
        q(g, "DisablePlayerControlsAltEx 4");
        assert_eq!(q(g, "SetFunctionValue (GPCDEx 2 4)"), 1.0);
        assert_eq!(q(g, "SetFunctionValue (GPCDEx 2 8)"), 0.0);
        assert_eq!(q(g, "SetFunctionValue (GPCDEx 1 4)"), 1.0);
        q(g, "EnablePlayerControlsAltEx 4");
        assert_eq!(q(g, "SetFunctionValue (GPCDEx 2 4)"), 0.0);
        assert_eq!(q(g, "short s\nif GPCDEx 2 4 && GPCDEx 2 8\nlet s := 1\nendif\nSetFunctionValue 3"), 3.0, "legacy if with two calls compiles");
    });
}
