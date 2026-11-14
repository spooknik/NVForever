//! Independently authored CTDA records and live source-getter controls for #2388.
//! Uses only existing public baseline APIs; no process-global roots or game assets.
use crate::common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{GameData, cond, game::{Game, GameConfig, RunCtx}};
use fnv_script::{FunctionTable, vm::{ArgValue, Value, XVal}};
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};

const LOCAL_PERK: u32 = 0x800;
const PERK: u32 = 0x0100_0800;
const PERK_CONDITION: u16 = 0x05C3;
const SIGHTS_CONDITION: u16 = 0x177A;

struct Fixture { data: GameData, forms: FormStore, refs: RefTable }

fn fixture() -> Fixture {
    let leading = [tes4(), top(b"NPC_", &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]))].concat();
    let perk = record(b"PERK", 0, LOCAL_PERK, &[sub(b"EDID", z("OwnedConditionPerk")), sub(b"DATA", vec![0, 1, 3, 1, 0])]);
    let weapon = |id, name: &str, scoped| {
        let data = [&0i32.to_le_bytes()[..], &100i32.to_le_bytes(), &1.0f32.to_le_bytes(), &10i16.to_le_bytes(), &[1u8]].concat();
        let mut fields = vec![sub(b"EDID", z(name)), sub(b"DATA", data), sub(b"DNAM", vec![0; 204])];
        if scoped { fields.push(sub(b"MOD3", z("Owned\\scope.nif"))); }
        record(b"WEAP", 0, id, &fields)
    };
    let owned = [tes4(), top(b"PERK", &perk), top(b"WEAP", &[weapon(0x801, "OwnedIronWeapon", false), weapon(0x802, "OwnedScopedWeapon", true)].concat())].concat();
    let plugins = [Plugin::parse(&leading, "OwnedLeading.esm".into()).unwrap(), Plugin::parse(&owned, "OwnedConditions.esm".into()).unwrap()];
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

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{}}\n{body}\nEnd"), RunCtx::default()).unwrap()
}

fn source(g: &mut Game, call: &str) -> f64 {
    let XVal::Num(value) = q(g, &format!("SetFunctionValue ({call})")) else { panic!("a numeric source control is required") };
    value
}

fn condition(g: &Game, function: u16, raw_perk: u32) -> cond::Condition {
    // Real public CTDA parser, including a deliberately nonidentity plugin FormID map.
    cond::parse(&serde_json::json!({
        "type": 0, "function": function, "comparison_value_raw": 1.0f32.to_bits(),
        "parameter1_raw": raw_perk, "parameter2_raw": 0, "run_on": 0,
    }), &g.data.param_types, &|raw| (raw == LOCAL_PERK).then_some(PERK)).unwrap()
}

fn value(g: &mut Game, c: &cond::Condition) -> f64 {
    g.eval_conditions(std::slice::from_ref(c), Some(FormId::PLAYER_REF), None, "owned CTDA")[0].value
}

#[test]
fn perk_rank_ctda_tracks_source_rank_and_maps_its_perk_form() {
    let f = fixture();
    let mut g = game(&f);
    let c = condition(&g, PERK_CONDITION, LOCAL_PERK);
    assert_eq!(c.opcode(), 0x15C3);
    for rank in [0, 1, 2, 0] {
        if rank == 0 { q(&mut g, "PlayerREF.RemovePerk OwnedConditionPerk"); }
        else { q(&mut g, &format!("PlayerREF.SetPerkRank OwnedConditionPerk {rank}")); }
        let expected = source(&mut g, "PlayerREF.GetPerkRank OwnedConditionPerk");
        assert_eq!(expected, f64::from(rank), "owned source state must really change");
        assert_eq!(value(&mut g, &c), expected, "CTDA must follow actual rank {rank}");
        assert_eq!(g.conditions_pass(std::slice::from_ref(&c), Some(FormId::PLAYER_REF), None, "rank compare"), rank == 1);
    }
    assert!(matches!(c.params.as_slice(), [ArgValue::Value(Value::Ref(p))] if *p == PERK), "perk parameter must be mapped as a form, not left as a local integer");
    let null = condition(&g, PERK_CONDITION, 0);
    assert_eq!(value(&mut g, &null), 0.0, "the existing null-form condition rule stays ahead of dispatch");
    assert!(!g.events.iter().any(|e| matches!(e.kind, fnv_game::trace::EventKind::CallFailed { .. })), "valid source/CTDA and null control must not fail a call");
    assert_eq!(g.unhandled.get(&0x15C3), None);
    assert_eq!(source(&mut game(&f), "PlayerREF.GetPerkRank OwnedConditionPerk"), 0.0, "another game keeps no rank");
}

#[test]
fn iron_sights_ctda_tracks_aiming_and_weapon_state_without_unknown_fallback() {
    let f = fixture();
    let mut g = game(&f);
    let c = condition(&g, SIGHTS_CONDITION, 0);
    assert_eq!(c.opcode(), 0x277A);
    q(&mut g, "PlayerREF.AddItem OwnedIronWeapon 1\nPlayerREF.AddItem OwnedScopedWeapon 1");
    for (weapon, aiming, expected) in [(None, false, 0.0), (None, true, 0.0), (Some("OwnedIronWeapon"), false, 0.0), (Some("OwnedIronWeapon"), true, 1.0), (Some("OwnedScopedWeapon"), true, 0.0), (Some("OwnedIronWeapon"), true, 1.0)] {
        if let Some(weapon) = weapon { q(&mut g, &format!("PlayerREF.EquipItem {weapon}")); }
        g.jip2.told.aiming = aiming;
        assert_eq!(source(&mut g, "GetPCUsingIronSights"), expected, "owned source weapon/aim control");
        assert_eq!(value(&mut g, &c), expected, "CTDA weapon {weapon:?}, aiming {aiming}");
        assert_eq!(g.conditions_pass(std::slice::from_ref(&c), Some(FormId::PLAYER_REF), None, "sights compare"), expected == 1.0);
    }
    assert!(c.params.is_empty(), "the no-argument condition must not decode phantom form parameters");
    assert_eq!(g.unhandled.get(&0x277A), None);
    let unknown = condition(&g, 0x6FFF, 0);
    assert_eq!(value(&mut g, &unknown), 0.0);
    assert_eq!(g.unhandled.get(&0x7FFF), Some(&1), "unrelated unknown opcode still follows the old dispatcher");
}
