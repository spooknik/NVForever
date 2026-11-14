//! Independent synthetic scene placement; no native assets or installed mod text.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    GameData,
    game::{Game, GameConfig},
};
use fnv_script::{FunctionTable, vm::XVal};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}
fn fixture() -> Fixture {
    let mut bytes = tes4();
    bytes.extend(top(b"NPC_", &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))])));
    bytes.extend(top(b"STAT", &record(b"STAT", 0, 0xD10, &[sub(b"EDID", z("OwnedSceneBase")), sub(b"MODL", z("owned\\original.nif"))])));
    bytes.extend(top(b"STAT", &record(b"STAT", 0, 0xD13, &[sub(b"EDID", z("OwnedOtherSceneBase")), sub(b"MODL", z("owned\\other.nif"))])));
    bytes.extend(top(b"PROJ", &record(b"PROJ", 0, 0xD20, &[sub(b"EDID", z("OwnedProjectileBase")), sub(b"MODL", z("owned\\projectile.nif"))])));
    bytes.extend(top(b"MISC", &record(b"MISC", 0, 0xD30, &[sub(b"EDID", z("OwnedInventoryBase")), sub(b"DATA", [25u32.to_le_bytes(), 1.0f32.to_le_bytes()].concat())])));
    bytes.extend(top(b"ACTI", &record(b"ACTI", 0, 0xD40, &[sub(b"EDID", z("OwnedActivatorBase"))])));
    let mut script_subs = vec![sub(b"EDID", z("OwnedSceneScript"))];
    script_subs.extend(unit(0, &script(&[(0, None, Vec::new())]), &[], &[]));
    bytes.extend(top(b"SCPT", &record(b"SCPT", 0, 0xD12, &script_subs)));
    bytes.extend(top(b"ACTI", &record(b"ACTI", 0, 0xD11, &[sub(b"EDID", z("OwnedScriptedBase")), sub(b"SCRI", u32s(&[0xD12]))])));
    let plugins = vec![Plugin::parse(&bytes, "OwnedSceneClone.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&functions).unwrap(), &json, &refs);
    Fixture { data, forms, refs }
}
fn game(f: &Fixture) -> Game<'_> {
    Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() })
}
fn place(g: &mut Game, temporary: bool) -> FormId {
    let prepare = if temporary { "let ownBase := TempCloneForm OwnedSceneBase 1\nSetModelPath \"owned\\replacement.nif\" ownBase\n" } else { "let ownBase := OwnedSceneBase\n" };
    let text = format!("ref ownBase\nref ownPlaced\nBegin Function {{}}\n{prepare}let ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd");
    let XVal::Ref(id) = g.run_query(&text, Default::default()).unwrap() else { panic!("expected reference result") };
    assert_ne!(id, 0, "a supported scene base produces a real reference");
    FormId(id)
}

fn save_result(f: &Fixture, script: &str) -> serde_json::Value {
    let input = serde_json::from_value(serde_json::json!({"format":"fnv.script.actions/1","new_game":false,"actions":[
        {"op":"query","label":"place","script":script},
        {"op":"save_load","label":"snapshot"}
    ]}))
    .unwrap();
    let trace = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    trace["observations"].as_array().unwrap().iter().find(|r| r["label"] == "snapshot").unwrap().clone()
}

#[test]
fn temporary_scene_placement_preserves_the_cloned_base_identity() {
    let f = fixture();
    let mut g = game(&f);
    let placed = place(&mut g, true);
    let base = g.world.peek(placed).unwrap().base;
    assert_ne!(base, FormId(0xD10), "silently placing the original loses the clone");
    assert_eq!(g.unclone(base), FormId(0xD10));
    assert_eq!(g.world.peek(placed).unwrap().count, 1);
}

#[test]
fn ordinary_scene_placement_keeps_its_original_base_and_snapshot() {
    let f = fixture();
    let mut g = game(&f);
    let placed = place(&mut g, false);
    assert_eq!(g.world.peek(placed).unwrap().base, FormId(0xD10));
    let snapshot = g.world.snapshot();
    assert_eq!(snapshot.refs.get(&placed).unwrap().base, FormId(0xD10));
    let restored = World::restore(&f.forms, &f.refs, &snapshot).unwrap();
    assert_eq!(restored.peek(placed).unwrap().base, FormId(0xD10));
}

#[test]
fn actual_headless_save_boundary_rejects_scene_clones_and_keeps_ordinary_saves() {
    let f = fixture();
    for temporary in [false, true] {
        let prepare = if temporary { "let ownBase := TempCloneForm OwnedSceneBase 1" } else { "let ownBase := OwnedSceneBase" };
        let script = format!("ref ownBase\nref ownPlaced\nBegin Function {{}}\n{prepare}\nlet ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd");
        let result = save_result(&f, &script);
        if temporary {
            assert!(result["errors"]["action"].as_str().is_some_and(|e| e.contains("temporary cloned base")), "{result}");
            assert!(result.get("bytes").is_none(), "no successful save-size answer after refusal");
        } else {
            assert!(result.get("errors").is_none(), "{result}");
            assert!(result["bytes"].as_u64().is_some_and(|n| n > 0));
        }
    }
}

#[test]
fn unsupported_scene_clone_shape_does_not_silently_place_an_original() {
    let f = fixture();
    let mut g = game(&f);
    let source = "ref ownBase\nref ownPlaced\nBegin Function {}\nlet ownBase := TempCloneForm Player 1\nlet ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd";
    assert_eq!(g.run_query(source, Default::default()).unwrap(), XVal::Ref(0));
    assert!(g.events.iter().any(|e| matches!(&e.kind,fnv_game::trace::EventKind::CallFailed {error,..} if error.contains("supported temporary scene base"))));
}

#[test]
fn scene_clone_uses_its_preplacement_override_and_refuses_late_replacement() {
    let f = fixture();
    let mut g = game(&f);
    let placed = place(&mut g, true);
    let base = g.world.peek(placed).unwrap().base;
    let read = fnv_game::nvse::call(&mut g, &Default::default(), "getmodelpath", Some(placed), &[]).unwrap().unwrap();
    assert_eq!(read, XVal::Str("owned\\replacement.nif".into()));
    let changed = fnv_game::nvse::call(&mut g, &Default::default(), "setmodelpath", None, &[XVal::Str("owned\\late.nif".into()), XVal::Ref(base.0)]).unwrap();
    assert!(changed.is_err(), "the preview must not claim a reload it cannot apply");
    assert_eq!(fnv_game::nvse::call(&mut g, &Default::default(), "getmodelpath", Some(placed), &[]).unwrap().unwrap(), read);
}

#[test]
fn deleting_a_scene_clone_keeps_the_save_refusal_for_its_tombstone() {
    let f = fixture();
    let mut g = game(&f);
    let placed = place(&mut g, true);
    let base = g.world.peek(placed).unwrap().base;
    g.world.set_deleted(placed, true).unwrap();
    let raw = g.snapshot();
    assert!(!raw.sim.refs.contains_key(&placed));
    assert_eq!(raw.sim.deleted_created.get(&placed), Some(&base));
    let source = "ref ownBase\nref ownPlaced\nBegin Function {}\nlet ownBase := TempCloneForm OwnedSceneBase 1\nlet ownPlaced := player.PlaceAtMe ownBase\nownPlaced.MarkForDelete\nSetFunctionValue ownPlaced\nEnd";
    let result = save_result(&f, source);
    assert!(result["errors"]["action"].as_str().is_some_and(|error| error.contains("temporary cloned base")), "{result}");
    assert!(result.get("bytes").is_none());
}

#[test]
fn a_script_attached_to_a_scene_clone_is_not_silently_dropped_by_placement() {
    let f = fixture();
    for setup in ["let ownBase := TempCloneForm OwnedSceneBase 1\nSetScript OwnedSceneScript ownBase", "let ownBase := TempCloneForm OwnedScriptedBase 1"] {
        let mut g = game(&f);
        assert_eq!(g.base_script(FormId(0xD11)), Some(FormId(0xD12)), "the inherited-script control must be real");
        let source = format!("ref ownBase\nref ownPlaced\nBegin Function {{}}\n{setup}\nlet ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd");
        assert_eq!(g.run_query(&source, Default::default()).unwrap(), XVal::Ref(0));
        assert_eq!(g.world.created().count(), 0);
        assert!(g.events.iter().any(|e| matches!(&e.kind,fnv_game::trace::EventKind::CallFailed {error,..} if error.contains("attached script"))));
    }
}

#[test]
fn an_inherited_original_model_override_is_frozen_at_placement_including_empty() {
    let f = fixture();
    for path in ["owned\\changed.nif", ""] {
        let mut g = game(&f);
        let source =
            format!("ref ownBase\nref ownPlaced\nBegin Function {{}}\nSetModelPath \"{path}\" OwnedSceneBase\nlet ownBase := TempCloneForm OwnedSceneBase 1\nlet ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd");
        let XVal::Ref(id) = g.run_query(&source, Default::default()).unwrap() else { panic!("expected reference") };
        assert_ne!(id, 0);
        let placed = FormId(id);
        let base = g.world.peek(placed).unwrap().base;
        let read = fnv_game::nvse::call(&mut g, &Default::default(), "getmodelpath", Some(placed), &[]).unwrap().unwrap();
        assert_eq!(read, XVal::Str(path.into()));
        assert_eq!(g.world.temporary_scene_base(base).unwrap().model_override.as_deref(), Some(path), "the model shared by drawing and physics must match the effective getter");
        let later = fnv_game::nvse::call(&mut g, &Default::default(), "setmodelpath", None, &[XVal::Str("owned\\late.nif".into()), XVal::Ref(0xD10)]).unwrap();
        assert!(later.is_err(), "original mutations cannot silently diverge from its placed aliases");
        assert_eq!(fnv_game::nvse::call(&mut g, &Default::default(), "getmodelpath", Some(placed), &[]).unwrap().unwrap(), read);
        assert!(fnv_game::nvse::call(&mut g, &Default::default(), "setmodelpath", None, &[XVal::Str("owned\\unrelated.nif".into()), XVal::Ref(0xD13)]).unwrap().is_ok());
    }
}

#[test]
fn inventory_clone_and_clone_chain_are_explicitly_refused() {
    let f = fixture();
    assert!(f.forms.item(FormId(0xD30)).is_some(), "the inventory control must really be an item");
    for prepare in ["let ownBase := TempCloneForm OwnedInventoryBase 1", "let ownFirst := TempCloneForm OwnedSceneBase 1\nlet ownBase := TempCloneForm ownFirst 1"] {
        let mut g = game(&f);
        let source = format!("ref ownFirst\nref ownBase\nref ownPlaced\nBegin Function {{}}\n{prepare}\nlet ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd");
        assert_eq!(g.run_query(&source, Default::default()).unwrap(), XVal::Ref(0));
        assert_eq!(g.world.created().count(), 0);
        assert!(g.events.iter().any(|e| matches!(&e.kind,fnv_game::trace::EventKind::CallFailed {error,..} if error.contains("supported temporary scene base") || error.contains("clone of a temporary clone"))));
    }
}

#[test]
fn a_projectile_clone_can_be_an_inert_created_model_but_not_an_activator() {
    let f = fixture();
    let mut g = game(&f);
    let source =
        "ref ownBase\nref ownPlaced\nBegin Function {}\nlet ownBase := TempCloneForm OwnedProjectileBase 1\nSetModelPath \"owned\\projectile-visual.nif\" ownBase\nlet ownPlaced := player.PlaceAtMe ownBase\nSetFunctionValue ownPlaced\nEnd";
    let XVal::Ref(id) = g.run_query(source, Default::default()).unwrap() else { panic!("expected reference") };
    assert_ne!(id, 0);
    let state = g.world.peek(FormId(id)).unwrap();
    assert!(!state.shot);
    assert_eq!(g.unclone(state.base), FormId(0xD20));
    assert_eq!(g.world.temporary_scene_base(state.base).unwrap().model_override.as_deref(), Some("owned\\projectile-visual.nif"));
    let mut g = game(&f);
    let source = "ref ownBase\nBegin Function {}\nlet ownBase := TempCloneForm OwnedActivatorBase 1\nSetFunctionValue (player.PlaceAtMe ownBase)\nEnd";
    assert_eq!(g.run_query(source, Default::default()).unwrap(), XVal::Num(0.0));
    assert_eq!(g.world.created().count(), 0, "activator behavior is not admitted by a model-only preview");
    assert!(g.events.iter().any(|e| matches!(&e.kind,fnv_game::trace::EventKind::CallFailed {error,..} if error.contains("supported temporary scene base"))));
}

#[test]
fn late_script_changes_to_a_placed_alias_or_original_are_refused() {
    let f = fixture();
    let mut g = game(&f);
    let placed = place(&mut g, true);
    let base = g.world.peek(placed).unwrap().base;
    for target in [base, FormId(0xD10)] {
        let result = fnv_game::nvse::call(&mut g, &Default::default(), "setscript", None, &[XVal::Ref(0xD12), XVal::Ref(target.0)]).unwrap();
        assert!(result.is_err(), "a late attachment must not silently change script behavior");
        assert_eq!(g.base_script(target), None);
    }
    assert!(fnv_game::nvse::call(&mut g, &Default::default(), "setscript", None, &[XVal::Ref(0xD12), XVal::Ref(0xD13)]).unwrap().is_ok());
    assert_eq!(g.base_script(FormId(0xD13)), Some(FormId(0xD12)));
}
