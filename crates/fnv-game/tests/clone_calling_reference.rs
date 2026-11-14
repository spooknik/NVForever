//! Owned caller/base controls; no installed plugin data or model getter dependency.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{GameData, game::{Game, GameConfig, RunCtx}};
use fnv_script::{FunctionTable, vm::XVal};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }
fn fixture() -> Fixture {
    let mut bytes = tes4();
    bytes.extend(top(b"NPC_", &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))])));
    bytes.extend(top(b"ACTI", &[
        record(b"ACTI", 0, 0xD00, &[sub(b"EDID", z("OwnedFirstBase"))]),
        record(b"ACTI", 0, 0xD01, &[sub(b"EDID", z("OwnedSecondBase"))]),
    ].concat()));
    let plugins = vec![Plugin::parse(&bytes, "OwnedCloneCaller.esm".into()).unwrap()];
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
fn q(g: &mut Game, source: &str) -> XVal {
    g.run_query(&format!("Begin Function {{}}\n{source}\nEnd"), RunCtx::default()).unwrap_or_else(|e| panic!("{source}: {e}"))
}

#[test]
fn omitted_clone_form_uses_the_calling_references_base() {
    let f = fixture(); let mut g = game(&f);
    let XVal::Ref(clone) = q(&mut g, "ref ownCaller\nlet ownCaller := player.PlaceAtMe OwnedFirstBase\nSetFunctionValue (ownCaller.TempCloneForm)") else { panic!("clone result") };
    assert_ne!(clone, 0, "a valid calling reference supplies the omitted base");
    assert_ne!(clone, 0xD00, "the clone keeps a fresh identity");
    assert_eq!(g.unclone(FormId(clone)), FormId(0xD00));
    assert_eq!(g.unclone(FormId(0xD00)), FormId(0xD00), "the original remains independent");
}

#[test]
fn explicit_clone_form_still_overrides_the_calling_reference() {
    let f = fixture(); let mut g = game(&f);
    let XVal::Ref(clone) = q(&mut g, "ref ownCaller\nlet ownCaller := player.PlaceAtMe OwnedFirstBase\nSetFunctionValue (ownCaller.TempCloneForm OwnedSecondBase 1)") else { panic!("clone result") };
    assert_ne!(clone, 0);
    assert_ne!(clone, 0xD01);
    assert_eq!(g.unclone(FormId(clone)), FormId(0xD01));
}

#[test]
fn no_form_and_no_calling_reference_still_returns_no_clone() {
    let f = fixture(); let mut g = game(&f);
    assert_eq!(q(&mut g, "SetFunctionValue (TempCloneForm)"), XVal::Ref(0));
}

#[test]
fn an_explicit_null_base_keeps_its_existing_no_clone_result() {
    let f = fixture(); let mut g = game(&f);
    assert_eq!(q(&mut g, "ref ownCaller\nlet ownCaller := player.PlaceAtMe OwnedFirstBase\nSetFunctionValue (ownCaller.TempCloneForm 0)"), XVal::Ref(0));
}
