//! Actual source registration and event dispatch over authored data, not installed mods.
mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_ui::MemSource;

const CELL: u32 = 0x900;
const ACTOR_A: FormId = FormId(0x901);
const ACTOR_B: FormId = FormId(0x902);
const EMPTY: u32 = 0x910;
const POPULATED: u32 = 0x911;
const WRONG: u32 = 0x912;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let npc = record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("OwnedRoom")), sub(b"DATA", vec![1])]);
    let actor = |id, name| record(b"ACHR", 0, id, &[sub(b"EDID", z(name)), sub(b"NAME", u32s(&[7])), sub(b"DATA", vec![0; 24])]);
    let refs = [actor(ACTOR_A.0, "OwnedActorA"), actor(ACTOR_B.0, "OwnedActorB")].concat();
    let cell = [cell, group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &refs))].concat();
    let lists = [
        record(b"FLST", 0, EMPTY, &[sub(b"EDID", z("OwnedEmptyList"))]),
        record(b"FLST", 0, POPULATED, &[sub(b"EDID", z("OwnedPopulatedList")), sub(b"LNAM", u32s(&[ACTOR_A.0]))]),
    ].concat();
    let wrong = record(b"STAT", 0, WRONG, &[sub(b"EDID", z("OwnedWrongType"))]);
    let bytes = [tes4(), top(b"NPC_", &npc), top(b"FLST", &lists), top(b"STAT", &wrong), top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)))].concat();
    let plugins = [Plugin::parse(&bytes, "OwnedListEvents.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalogue = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalogue, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let mut game = Game::new(&f.data, World::new(&f.forms, &f.refs, SimConfig::default()), GameConfig { strict: true, ..Default::default() });
    game.player_entered(Some(FormId(CELL)));
    let ui = MemSource::default().with("owned.xml", r#"<menu name="HUDMainMenu"><rect name="Probe"><_selected>0</_selected><_neighbor>0</_neighbor><_registered>0</_registered></rect></menu>"#);
    game.ui_tree.load_menu(&ui, "owned.xml").unwrap();
    game.ui_tree.update();
    game
}

fn register(game: &mut Game, key: &str, filter_setup: &str) -> XVal {
    let source = format!(
        "ref rFilter\nfloat i\nfloat iRegistered\nBegin Function {{ }}\n{filter_setup}\nlet iRegistered := SetOnAnimActionEventHandler (Begin Function {{int action}}\nSetUIFloat \"HUDMainMenu/Probe/_{key}\" ((GetUIFloat \"HUDMainMenu/Probe/_{key}\") + 1)\nEnd) 1 rFilter 9\nSetUIFloat \"HUDMainMenu/Probe/_registered\" iRegistered\nSetFunctionValue rFilter\nEnd\n"
    );
    let value = game.run_query(&source, RunCtx::default()).unwrap_or_else(|error| panic!("{source}: {error}"));
    assert!(matches!(value, XVal::Ref(id) if id != 0), "the fixture must register a non-null reference filter: {value:?}");
    assert_eq!(calls(game, "registered"), 1.0, "the actual registration API must report that it accepted the handler");
    value
}

fn neighbor(game: &mut Game) {
    register(game, "neighbor", "let rFilter := OwnedEmptyList");
}

fn fire(game: &mut Game, on: FormId) -> usize {
    game.jip_event("animaction", Some(on), vec![XVal::Num(9.0)])
}

fn calls(game: &Game, key: &str) -> f64 {
    game.ui_tree.get_float(&format!("HUDMainMenu/Probe/_{key}")).unwrap()
}

fn assert_clean(game: &Game) {
    assert!(!game.events.iter().any(|event| matches!(event.kind, EventKind::CallFailed { .. } | EventKind::ScriptError { .. })), "valid fixture callbacks must run without hidden failures");
}

#[test]
fn ordinary_empty_list_keeps_its_existing_wildcard_dispatch() {
    let f = fixture(); let mut game = game(&f);
    neighbor(&mut game);
    register(&mut game, "selected", "let rFilter := OwnedEmptyList");
    assert_eq!(fire(&mut game, ACTOR_A), 2);
    assert_eq!(fire(&mut game, ACTOR_B), 2);
    assert_eq!(game.jip_event("animaction", Some(ACTOR_A), vec![XVal::Num(10.0)]), 0, "the action filter must remain selective");
    assert_eq!((calls(&game, "selected"), calls(&game, "neighbor")), (2.0, 2.0));
    assert_clean(&game);
}

#[test]
fn explicitly_cleared_clone_matches_the_same_actor_events_as_an_ordinary_empty_list() {
    let f = fixture(); let mut game = game(&f);
    neighbor(&mut game);
    let filter = register(&mut game, "selected", "let rFilter := TempCloneForm OwnedPopulatedList 0\nListClear rFilter");
    assert_ne!(filter, XVal::Ref(POPULATED));
    assert_eq!(fnv_game::functions::list_members(&game, FormId(POPULATED)), [ACTOR_A], "clearing the clone must not clear its original");
    assert_eq!(fire(&mut game, ACTOR_A), 2);
    assert_eq!(fire(&mut game, ACTOR_B), 2);
    assert_eq!((calls(&game, "selected"), calls(&game, "neighbor")), (2.0, 2.0));
    assert_clean(&game);
}

#[test]
fn cleared_clone_chains_have_no_arbitrary_type_resolution_depth_limit() {
    let f = fixture(); let mut game = game(&f);
    neighbor(&mut game);
    register(&mut game, "selected", "let rFilter := OwnedPopulatedList\nwhile i < 70\nlet rFilter := TempCloneForm rFilter 0\nlet i := i + 1\nloop\nListClear rFilter");
    assert_eq!(fire(&mut game, ACTOR_B), 2);
    assert_eq!((calls(&game, "selected"), calls(&game, "neighbor")), (1.0, 1.0));
    assert_clean(&game);
}

#[test]
fn materialized_nonempty_clone_uses_membership_and_never_becomes_universal() {
    let f = fixture(); let mut game = game(&f);
    neighbor(&mut game);
    register(&mut game, "selected", "let rFilter := TempCloneForm OwnedPopulatedList 0\nListClear rFilter\nListAddForm rFilter OwnedActorA");
    assert_eq!(fire(&mut game, ACTOR_A), 2);
    assert_eq!(fire(&mut game, ACTOR_B), 1);
    assert_eq!((calls(&game, "selected"), calls(&game, "neighbor")), (1.0, 2.0));
    assert_clean(&game);
}

#[test]
fn an_unmaterialized_nonempty_original_clone_is_not_mistaken_for_an_empty_list() {
    let f = fixture(); let mut game = game(&f);
    neighbor(&mut game);
    register(&mut game, "selected", "let rFilter := TempCloneForm OwnedPopulatedList 0");
    // This is solely a non-member safety boundary. It makes no claim that copied
    // membership of the untouched clone is implemented for ACTOR_A.
    assert_eq!(fire(&mut game, ACTOR_B), 1);
    assert_eq!((calls(&game, "selected"), calls(&game, "neighbor")), (0.0, 1.0));
    assert_clean(&game);
}

#[test]
fn a_cleared_wrong_type_clone_is_not_a_form_list_wildcard_and_neighbors_still_run() {
    let f = fixture(); let mut game = game(&f);
    neighbor(&mut game);
    register(&mut game, "selected", "let rFilter := TempCloneForm OwnedWrongType 0\nListClear rFilter");
    assert_eq!(fire(&mut game, ACTOR_A), 1);
    assert_eq!(fire(&mut game, ACTOR_B), 1);
    assert_eq!((calls(&game, "selected"), calls(&game, "neighbor")), (0.0, 2.0));
    assert_clean(&game);
}
