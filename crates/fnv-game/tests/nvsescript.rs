//! NVSEScript units run (#2275; script-compiler.md §8.2): the extender's second language,
//! compiled through the first grammar, gives the values its page documents.

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let bytes = [tes4()].concat();
    let plugins = vec![Plugin::parse(&bytes, "NvseScript.esm".into()).unwrap()];
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

fn run(body: &str) -> XVal {
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    let src = format!("name Probe;\n\nfn () {{\n{body}\n}}\n");
    g.run_query(&src, RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn num(body: &str) -> f64 {
    match run(body) {
        XVal::Num(x) => x,
        v => panic!("{body}: {v:?}"),
    }
}

#[test]
fn a_for_loop_runs_its_increment_before_continue() {
    // [documented: NVSEScript, For loops.]
    let body = "int s = 0;\nfor (int i = 0; i < 5; i++) {\n  if (i == 2) { continue; }\n  s += i;\n}\nreturn s;";
    assert_eq!(num(body), 8.0);
}

#[test]
fn for_in_takes_the_elements_of_an_array_and_a_map() {
    // [documented: NVSEScript, For-each loops; Array/Map literals.]
    assert_eq!(num("array a = [1, 2, 3];\nint t = 0;\nfor ([int e] in a) {\n  t += e;\n}\nreturn t;"), 6.0);
    assert_eq!(num("array m = {\"a\"::1, \"b\"::2};\nint t = 0;\nfor ([string k, int v] in m) { t += v; }\nreturn t;"), 3.0);
    assert_eq!(num("array m = {\"a\"::5};\nint t = 0;\nfor ([_, int v] in m) { t += v; }\nreturn t;"), 5.0);
}

#[test]
fn strings_take_expressions() {
    // [documented: NVSEScript, Interpolation.]
    match run("int n = 7;\nstring s = \"n=${n + 1}; done\";\nreturn s;") {
        XVal::Str(s) => assert_eq!(s, "n=8; done"),
        v => panic!("{v:?}"),
    }
}

#[test]
fn while_and_match() {
    // [documented: NVSEScript, While loops; Match statements.]
    let body = "int i = 0;\nint r = 0;\nwhile (i < 3) {\n  match (i) {\n    1 -> { r += 10; }\n    _ -> { r += 1; }\n  }\n  i++;\n}\nreturn r;";
    assert_eq!(num(body), 12.0);
}

#[test]
fn lambdas_are_called_through_their_variable() {
    // [documented: Lambda, `ref rCallback = fn (int iSomeParam) -> …;` and
    // `rCallback(10);`; the single-line form returns its value.]
    assert_eq!(num("ref f = fn (int x) -> x * 2;
return f(10) + f(5);"), 30.0);
    // A lambda writes its maker's variables [documented: Lambda, "They can access and
    // modify all variables defined in the parent script"].
    assert_eq!(num("int c = 1;
ref h = fn (int x) -> c += x;
h(10);
h(5);
return c;"), 16.0);
    assert_eq!(num("ref g = fn (int y) {
  return y * 3;
};
return g(4);"), 12.0);
}
