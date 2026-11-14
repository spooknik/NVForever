//! The actual Game error gate must admit a report before building its context.
#[path = "../tests/common/mod.rs"]
mod common;

use crate::game::{Game, GameConfig};
use crate::trace::EventKind;
use crate::GameData;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use std::cell::Cell;

#[test]
fn suppressed_unusable_reports_never_construct_context() {
    let bytes = common::tes4();
    let plugins = [Plugin::parse(&bytes, "OwnedContextGate.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let path = root.join("script/functions.json");
    let catalog = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, FunctionTable::load(&path).unwrap(), &catalog, &refs);
    let mut game = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig::default());
    let constructed = Cell::new(0);
    let unusable = "owned is not a function script";
    for prospective_registration in 0..4 {
        game.handler_failed_with_context("JIP hit handler".into(), unusable.into(), || {
            constructed.set(constructed.get() + 1);
            Some(format!("registration={prospective_registration}"))
        });
    }
    assert_eq!(constructed.get(), 1, "suppressed attempts must not evaluate their lazy context");
    game.handler_failed_with_context("JIP keydown handler".into(), unusable.into(), || {
        constructed.set(constructed.get() + 1);
        Some("a different handler kind".into())
    });
    assert_eq!(constructed.get(), 2, "the original dedup key includes the handler kind");
    for _ in 0..2 {
        game.handler_failed_with_context("JIP hit handler".into(), "an unknown lambda".into(), || {
            constructed.set(constructed.get() + 1);
            Some("ordinary error".into())
        });
    }
    assert_eq!(constructed.get(), 4, "ordinary failures retain their original frequency");
    let errors: Vec<_> = game.events.iter().filter_map(|event| match &event.kind {
        EventKind::ScriptError { unit, error } => Some((unit.as_str(), error.as_str())),
        _ => None,
    }).collect();
    assert_eq!(errors, vec![
        ("JIP hit handler", unusable),
        ("JIP keydown handler", unusable),
        ("JIP hit handler", "an unknown lambda"),
        ("JIP hit handler", "an unknown lambda"),
    ]);
}
