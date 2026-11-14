//! The game clock as scripts move it (#1377; simulation.md §5).

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{clock, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

fn glob(fid: u32, name: &str, v: f32) -> Vec<u8> {
    record(b"GLOB", 0, fid, &[sub(b"EDID", z(name)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", v.to_le_bytes().to_vec())])
}

fn plugin_bytes() -> Vec<u8> {
    let globals = [
        glob(0x901, "GameYear", 2281.0),
        glob(0x902, "GameMonth", 9.0),
        glob(0x903, "GameDay", 18.0),
        glob(0x904, "GameHour", 12.0),
        glob(0x905, "GameDaysPassed", 5.0),
        glob(0x906, "TimeScale", 30.0),
    ]
    .concat();
    [tes4(), top(b"GLOB", &globals)].concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "GameClock.esm".into()).unwrap()];
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
    Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() })
}

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

const FRAME: f64 = 1.0 / 60.0;

#[test]
fn set_game_hour_0_reaches_midnight_after_an_ordinary_frame() {
    let f = fixture();
    let mut g = game(&f);
    g.world.advance_real_seconds(FRAME).unwrap();
    q(&mut g, "SetGameHour 0");
    let c = g.world.calendar().unwrap();
    assert_eq!((c.day, c.hour), (19.0, 0.0));
    assert!((c.days_passed - 5.5).abs() < 1e-5, "{}", c.days_passed);
}

#[test]
fn set_game_hour_to_the_hour_shown_passes_no_day() {
    let f = fixture();
    let mut g = game(&f);
    g.world.advance_real_seconds(FRAME).unwrap();
    let before = g.world.calendar().unwrap();
    q(&mut g, &format!("SetGameHour {}", before.hour));
    assert_eq!(g.world.calendar().unwrap(), before);
    q(&mut g, "SetGameHour");
    assert_eq!(g.world.calendar().unwrap(), before);
}

#[test]
fn a_script_set_of_the_same_value_is_the_time() {
    let f = fixture();
    let mut g = game(&f);
    let (days, _) = g.world.forms.global_by_edid(clock::GAME_DAYS_PASSED).unwrap();
    g.world.set_global(days, 1000.0).unwrap();
    for _ in 0..4 {
        g.world.advance_real_seconds(FRAME).unwrap();
    }
    assert_eq!(g.world.global(days).unwrap(), 1000.0);
    q(&mut g, "Set GameDaysPassed to 1000");
    for _ in 0..2 {
        g.world.advance_real_seconds(FRAME).unwrap();
    }
    assert_eq!(g.world.global(days).unwrap(), 1000.0);
}

/// A script's write that moves GameHour more than an hour on the clock face is noted for
/// the engine's actors (#2353: VCG01 stage 200's `set GameHour to 8`); a small move, or a
/// write to another time global, is not; taking it clears it.
#[test]
fn a_script_jump_of_game_hour_is_noted_once() {
    let f = fixture();
    let mut g = game(&f);
    q(&mut g, "Set GameHour to 12.5");
    assert_eq!(g.take_hour_jump(), None, "half an hour is no jump");
    q(&mut g, "Set GameDaysPassed to 9");
    assert_eq!(g.take_hour_jump(), None, "not GameHour");
    q(&mut g, "Set GameHour to 8");
    let j = g.take_hour_jump().expect("four and a half hours back");
    assert_eq!((j.from, j.to), (12.5, 8.0));
    assert_eq!(g.take_hour_jump(), None, "taken");
    // Over midnight: 23:00 to 01:00 is two hours on the clock face.
    q(&mut g, "Set GameHour to 23");
    g.take_hour_jump();
    q(&mut g, "Set GameHour to 1");
    let j = g.take_hour_jump().expect("two hours over midnight");
    assert_eq!((j.from, j.to), (23.0, 1.0));
}

/// Several writes before the engine looks are one jump, from the first hour to the last;
/// one that comes back to about where it started is none.
#[test]
fn script_writes_between_two_looks_are_one_jump() {
    let f = fixture();
    let mut g = game(&f);
    q(&mut g, "Set GameHour to 18");
    q(&mut g, "Set GameHour to 20");
    let j = g.take_hour_jump().unwrap();
    assert_eq!((j.from, j.to), (12.0, 20.0));
    q(&mut g, "Set GameHour to 2");
    q(&mut g, "Set GameHour to 20.5");
    assert_eq!(g.take_hour_jump(), None);
}

/// JIP's SetGameHour moves the clock as a wait does: a jump too.
#[test]
fn set_game_hour_function_is_a_jump() {
    let f = fixture();
    let mut g = game(&f);
    q(&mut g, "SetGameHour 16");
    let j = g.take_hour_jump().expect("four hours on");
    assert!((j.from - 12.0).abs() < 1e-4 && (j.to - 16.0).abs() < 1e-4, "{j:?}");
}
