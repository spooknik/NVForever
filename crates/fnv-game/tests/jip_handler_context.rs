//! Owned synthetic dispatch controls; no installed game or native extender needed.
mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, LAMBDA_BASE};
use fnv_game::trace::EventKind;
use fnv_game::GameData;
use fnv_script::vm::{EventCall, XVal};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use fnv_ui::MemSource;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

type Fields = BTreeMap<String, String>;

#[derive(Clone, Default)]
struct Warnings(Arc<Mutex<Vec<Fields>>>);

struct Values(Fields);

impl Visit for Values {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.insert(field.name().into(), format!("{value:?}"));
    }
}

impl<S: Subscriber> Layer<S> for Warnings {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut values = Values(BTreeMap::new());
        event.record(&mut values);
        if values.0.contains_key("handler_context") {
            self.0.lock().unwrap().push(values.0);
        }
    }
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let bytes = [tes4(), top(b"NPC_", &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]))].concat();
    let plugins = vec![Plugin::parse(&bytes, "OwnedHandlers.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let catalog = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let table = FunctionTable::load(&path).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &catalog, &refs);
    Fixture { data, forms, refs }
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut game = Game::new(&f.data, world, GameConfig { strict: true, ..Default::default() });
    let source = MemSource::default().with("owned.xml", r#"<menu name="HUDMainMenu"><rect name="Probe"><_calls>0</_calls></rect></menu>"#);
    game.ui_tree.load_menu(&source, "owned.xml").unwrap();
    game.ui_tree.update();
    game
}

fn neighbor(game: &mut Game) -> XVal {
    game.run_query(
        "Begin Function { }\nSetFunctionValue (Begin Function { }\nSetUIFloat \"HUDMainMenu/Probe/_calls\" ((GetUIFloat \"HUDMainMenu/Probe/_calls\") + 1)\nEnd)\nEnd\n",
        Default::default(),
    ).unwrap()
}

fn register(game: &mut Game<'_>, func: XVal, filters: Vec<XVal>) {
    assert_eq!(game.event_call(EventCall::JipSet {
        event: "hit".into(), func, add: true, filters,
    }, "OwnedHandlers.esm").unwrap(), XVal::Num(1.0));
}

// Remove only after all dispatch/error/context assertions. Successful removal
// proves the failed dispatch retained that exact registration; removing again
// must report that it no longer exists.
fn assert_retained(game: &mut Game<'_>, func: XVal, filters: Vec<XVal>) {
    for expected in [1.0, 0.0] {
        assert_eq!(game.event_call(EventCall::JipSet {
            event: "hit".into(), func: func.clone(), add: false, filters: filters.clone(),
        }, "OwnedHandlers.esm").unwrap(), XVal::Num(expected));
    }
}

fn errors<'a>(game: &'a Game<'_>) -> Vec<(&'a str, &'a str)> {
    game.events.iter().filter_map(|event| match &event.kind {
        EventKind::ScriptError { unit, error } => Some((unit.as_str(), error.as_str())),
        _ => None,
    }).collect()
}

#[test]
fn invalid_callables_are_identified_without_stopping_a_valid_neighbor() {
    let f = fixture();
    let mut game = game(&f);
    let valid = neighbor(&mut game);
    let bad_a = LAMBDA_BASE + 90_000;
    let bad_b = LAMBDA_BASE + 90_001;
    let registrations = [valid, XVal::Ref(bad_a), XVal::Ref(bad_b)];
    for func in &registrations { register(&mut game, func.clone(), vec![XVal::Ref(0)]); }
    let warnings = Warnings::default();
    let subscriber = tracing_subscriber::registry().with(warnings.clone());
    tracing::subscriber::with_default(subscriber, || {
        assert_eq!(game.jip_event("hit", Some(FormId::PLAYER_REF), vec![]), 1);
        assert_eq!(game.jip_event("hit", Some(FormId::PLAYER_REF), vec![]), 1);
    });
    assert_eq!(game.ui_tree.get_float("HUDMainMenu/Probe/_calls"), Some(2.0));
    assert_eq!(errors(&game), vec![("JIP hit handler", "an unknown lambda"); 4]);
    let captured = warnings.0.lock().unwrap();
    assert_eq!(captured.len(), 4, "unknown lambda keeps its ordinary reporting frequency");
    for pair in captured.as_chunks::<2>().0 {
        assert!(pair[0]["handler_context"].contains(&format!("registration=3 callable=ref:0x{bad_b:08X}")));
        assert!(pair[1]["handler_context"].contains(&format!("registration=2 callable=ref:0x{bad_a:08X}")));
        assert!(pair[0]["handler_context"].contains("receiver=0x00000014 args=0 filters(total=1)=[ref:0x00000000]"));
        assert_eq!(pair[0]["frame"], "0");
        assert_eq!(pair[0]["unit"], "JIP hit handler");
        assert_eq!(pair[0]["error"], "an unknown lambda");
    }
    for func in registrations { assert_retained(&mut game, func, vec![XVal::Ref(0)]); }
}

#[test]
fn changing_context_does_not_bypass_unusable_error_dedup_or_skip_neighbors() {
    let f = fixture();
    let mut game = game(&f);
    let valid = neighbor(&mut game);
    // Distinct wildcard filter vectors are distinct public registrations, while
    // both match this hit. Identical tuples would be deduplicated at registration.
    let registrations = [(valid, vec![XVal::Ref(0)]),
        (XVal::Ref(0), vec![XVal::Ref(0)]), (XVal::Ref(0), vec![])];
    for (func, filters) in &registrations { register(&mut game, func.clone(), filters.clone()); }
    let warnings = Warnings::default();
    let subscriber = tracing_subscriber::registry().with(warnings.clone());
    tracing::subscriber::with_default(subscriber, || {
        for _ in 0..3 {
            assert_eq!(game.jip_event("hit", Some(FormId::PLAYER_REF), vec![]), 1);
        }
    });
    assert_eq!(game.ui_tree.get_float("HUDMainMenu/Probe/_calls"), Some(3.0));
    let errors = errors(&game);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].0, "JIP hit handler");
    assert_eq!(errors[0].1, "0x00000000 is not a function script");
    let captured = warnings.0.lock().unwrap();
    assert_eq!(captured.len(), 1, "original kind/error key also gates contextual warnings");
    assert!(captured[0]["handler_context"].contains("registration=3"));
    for (func, filters) in registrations { assert_retained(&mut game, func, filters); }
}

#[test]
fn actual_failure_context_bounds_unicode_strings_and_filters_without_reading_arrays() {
    let f = fixture();
    let mut game = game(&f);
    let text = format!("\n\r\t\0\"\\{}", "漢🦀".repeat(200));
    let func = XVal::Str(text.clone());
    let filters = vec![XVal::Ref(0), XVal::Str(text), XVal::Array(u32::MAX), XVal::Num(-2.5), XVal::Str("OMITTED-FILTER-SENTINEL".into())];
    register(&mut game, func.clone(), filters.clone());
    let warnings = Warnings::default();
    let subscriber = tracing_subscriber::registry().with(warnings.clone());
    tracing::subscriber::with_default(subscriber, || {
        assert_eq!(game.jip_event("hit", Some(FormId::PLAYER_REF), vec![]), 0);
    });
    assert_eq!(errors(&game), vec![("JIP hit handler", "Call needs a function script or a lambda")]);
    let captured = warnings.0.lock().unwrap();
    assert_eq!(captured.len(), 1);
    let context = &captured[0]["handler_context"];
    assert!(context.len() < 1600, "{} bytes: {context}", context.len());
    assert!(!context.contains(['\n', '\r', '\t', '\0']));
    assert!(context.contains("\\n\\r\\t\\0"));
    assert!(context.contains("漢🦀"));
    assert!(context.contains("<truncated>"));
    assert!(context.contains("filters(total=5)="));
    assert!(context.contains("array:4294967295"));
    assert!(context.contains("number:-2.5"));
    assert!(!context.contains("OMITTED-FILTER-SENTINEL"));
    assert_retained(&mut game, func, filters);
}
