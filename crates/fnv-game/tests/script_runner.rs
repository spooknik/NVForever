//! JIP's Script Runner (#1460, docs/spec-notes/ext-script-runner.md): files under
//! `Data\NVSE\plugins\scripts` run at the moment their prefix names, RegisterSRScript adds
//! files from subfolders while the game restart's run, and a file that does not compile is
//! skipped. A synthetic plugin and made-up files (no game data).

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::script_runner::Moment;
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

fn glob(fid: u32, edid: &str) -> Vec<u8> {
    record(b"GLOB", 0, fid, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())])
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let globs = [glob(0x800, "SROrder"), glob(0x801, "SRLoads"), glob(0x802, "SRNew"), glob(0x803, "SRSize"), glob(0x804, "SRRegistered"), glob(0x805, "SRLate")].concat();
    let bytes = [tes4(), top(b"GLOB", &globs)].concat();
    let plugins = vec![Plugin::parse(&bytes, "SR.esm".into()).unwrap()];
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

/// The Data folder of the run, written once per test process (the tests run in parallel).
fn files() {
    use fnv_game::mod_files::{set_roots, Roots};
    let dir = std::env::temp_dir().join(format!("nvf-script-runner-{}", std::process::id()));
    static WRITE: std::sync::Once = std::sync::Once::new();
    WRITE.call_once(|| {
        let scripts = dir.join("NVSE/Plugins/Scripts");
        std::fs::create_dir_all(scripts.join("optional")).unwrap();
        let w = |name: &str, text: &str| std::fs::write(scripts.join(name), text).unwrap();
        // Run in the order of their names: 1, then 2.
        w("gr_b.txt", "set SROrder to SROrder * 10 + 2\r\n");
        w("GR_a.txt", "set SROrder to SROrder * 10 + 1\r\n");
        // Inline declarations and an array that outlives the file.
        w("gr_c.txt", "let array_var aList := ar_List 1, 2, 3\r\nint iSize = ar_Size aList\r\nset SRSize to iSize\r\n");
        // Registers a subfolder's file; the same name again, a file at the top, a missing
        // file and one that does not compile are refused.
        w(
            "gr_d.txt",
            "set SRRegistered to RegisterSRScript \"optional\\gl_extra.txt\"\r\n\
             set SRRegistered to SRRegistered * 10 + RegisterSRScript \"optional\\gl_extra.txt\"\r\n\
             set SRRegistered to SRRegistered * 10 + RegisterSRScript \"gl_top.txt\"\r\n\
             set SRRegistered to SRRegistered * 10 + RegisterSRScript \"optional\\gl_missing.txt\"\r\n\
             set SRRegistered to SRRegistered * 10 + RegisterSRScript \"optional\\gl_broken.txt\"\r\n\
             set SRRegistered to SRRegistered * 10 + RegisterSRScript \"optional\\gr_late.txt\"\r\n",
        );
        w("optional/gl_extra.txt", "set SRLoads to SRLoads + 100\r\n");
        // Does not compile: Set without To (an unknown name alone is a call in a mod's file, #2275).
        w("optional/gl_broken.txt", "set SRLoads 1\r\n");
        // A registered gr_ file runs after the folder's gr_ files.
        w("optional/gr_late.txt", "set SRLate to SROrder\r\n");
        w("gl_top.txt", "set SRLoads to SRLoads + 1\r\n");
        w("ln_both.txt", "set SRLoads to SRLoads + 10\r\nset SRNew to SRNew + 10\r\n");
        w("gn_new.txt", "set SRNew to SRNew + 1\r\n");
        // Outside the game restart RegisterSRScript does nothing.
        w("gn_register.txt", "set SRNew to SRNew + 1000 * RegisterSRScript \"optional\\gl_extra.txt\"\r\n");
        // Skipped: does not compile; no known prefix; not a text file.
        w("gr_broken.txt", "set SROrder 99\r\n");
        w("readme.txt", "set SROrder to 99\r\n");
        w("gr_notes.ini", "set SROrder to 99\r\n");
        // A private auxiliary variable (no `_`) the file writes, which a CompileScript'd
        // function reads back (MCM Extender's pattern).
        w("gr_e.txt", "SRSize.AuxVarSetFlt \"*Shared\" 7 -1\r\n");
        let udfs = dir.join("NVSE/user_defined_functions");
        std::fs::create_dir_all(&udfs).unwrap();
        std::fs::write(udfs.join("srprobe.txt"), "Begin Function { }\r\nSetFunctionValue (SRSize.AuxVarGetFlt \"*Shared\" 0)\r\nEnd\r\n").unwrap();
    });
    set_roots(Roots { read: vec![dir], ..Roots::default() });
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    Game::new(&f.data, world, GameConfig::default())
}

fn value(g: &mut Game, expr: &str) -> f64 {
    match g.run_query(&format!("Begin Function {{ }}\nSetFunctionValue {expr}\nEnd\n"), Default::default()).unwrap() {
        XVal::Num(n) => n,
        v => panic!("{expr}: {v:?}"),
    }
}

#[test]
fn each_prefix_runs_at_its_moment() {
    files();
    let f = fixture();
    let mut g = game(&f);
    let runs = g.run_script_runner(Moment::Restart);
    let names: Vec<&str> = runs.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(names, ["GR_a.txt", "gr_b.txt", "gr_broken.txt", "gr_c.txt", "gr_d.txt", "gr_e.txt", "optional\\gr_late.txt"]);
    assert_eq!(value(&mut g, "SROrder"), 12.0, "the folder's gr_ files in the order of their names");
    assert_eq!(value(&mut g, "SRLate"), 12.0, "a registered gr_ file runs after them");
    assert_eq!(value(&mut g, "SRSize"), 3.0);
    assert_eq!(value(&mut g, "SRLoads") + value(&mut g, "SRNew"), 0.0, "no other prefix runs at the restart");
    let broken = runs.iter().find(|r| r.path == "gr_broken.txt").unwrap();
    assert!(broken.result.as_ref().is_err_and(|e| e.starts_with("line 1: ")), "{broken:?}");
    assert!(runs.iter().filter(|r| r.path != "gr_broken.txt").all(|r| r.result.is_ok()), "{runs:?}");

    g.run_script_runner(Moment::NewGame);
    assert_eq!(value(&mut g, "SRNew"), 11.0, "gn_ and ln_ at a new game; RegisterSRScript only at the restart");
    assert_eq!(value(&mut g, "SRLoads"), 10.0, "ln_ at a new game, gl_ not");

    g.run_script_runner(Moment::Load);
    // ln_ 10 more, gl_top 1, the registered gl_extra 100.
    assert_eq!(value(&mut g, "SRLoads"), 121.0);
    assert_eq!(value(&mut g, "SROrder"), 12.0, "gr_ files run once");
}

#[test]
fn register_sr_script_takes_subfolder_files_once() {
    files();
    let f = fixture();
    let mut g = game(&f);
    g.run_script_runner(Moment::Restart);
    // Registered; the same name again 0; a file at the top 0; missing 0; does not compile 0;
    // the gr_ file 1.
    assert_eq!(value(&mut g, "SRRegistered"), 100001.0);
}

#[test]
fn registered_files_last_for_the_session() {
    files();
    let f = fixture();
    let mut g = game(&f);
    g.run_script_runner(Moment::Restart);
    let session = g.take_session();
    let mut g2 = game(&f);
    g2.put_session(session);
    g2.run_script_runner(Moment::Load);
    assert_eq!(value(&mut g2, "SRLoads"), 111.0, "gl_top, ln_both and the registered gl_extra");
}

#[test]
fn script_runner_files_and_compiled_scripts_share_private_aux_vars() {
    // [hypothesis: scripts compiled at run time have no plugin of their own; observed: MCM
    // Extender's gr_ file writes `*MCMExt_ModName`, its CompileScript'd handler reads it,
    // Begin Again list 2026-11-13]
    files();
    let f = fixture();
    let mut g = game(&f);
    g.run_script_runner(Moment::Restart);
    assert_eq!(value(&mut g, "SRSize.AuxVarGetFlt \"*Shared\" 0"), 7.0, "a query reads it");
    assert_eq!(value(&mut g, "Call (CompileScript \"srprobe.txt\")"), 7.0, "so does a CompileScript'd function");
}
