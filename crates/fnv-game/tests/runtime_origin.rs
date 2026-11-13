//! Owned AuxVar runtime-origin controls. No game/mod source or data.
//! Standalone: this process owns mod_files::ROOTS for the entire binary.
mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::GameData;
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::script_runner::Moment;
use fnv_game::trace::EventKind;
use fnv_script::compile::{Env, FormInfo};
use fnv_script::unit::RefSlot;
use fnv_script::vm::XVal;
use fnv_script::{FunctionTable, VarDecl, VarType};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

struct Names;
impl Env for Names {
    fn form(&self, _: &str) -> Option<FormInfo> {
        None
    }
    fn info(&self, _: u32) -> Option<FormInfo> {
        None
    }
    fn editor_id(&self, _: u32) -> Option<String> {
        None
    }
    fn local_form(&self, local: u32) -> Option<u32> {
        Some(local)
    }
    fn script_vars(&self, _: u32) -> Option<Vec<VarDecl>> {
        None
    }
}

fn function(table: &FunctionTable, id: u32, name: &str, setter: bool) -> Vec<u8> {
    let declarations = if setter { "float amount\n" } else { "" };
    let parameters = if setter { "amount" } else { "" };
    let writes = if setter {
        "player.AuxVarSetFlt \"*OwnOriginPrivate\" amount\nplayer.AuxVarSetFlt \"*_OwnOriginShared\" amount\n"
    } else {
        ""
    };
    let src = format!(
        "scn {name}\n{declarations}Begin Function {{{parameters}}}\n{writes}SetFunctionValue (player.AuxVarGetFlt \"*OwnOriginPrivate\")\nEnd\n"
    );
    let compiled = fnv_script::compile::compile(&src, table, &Names, None);
    assert!(
        compiled.errors().next().is_none(),
        "{}: {:?}",
        name,
        compiled.diags
    );
    let vars: Vec<_> = compiled
        .variables
        .iter()
        .map(|v| (v.index, v.name.as_str(), u8::from(v.ty == VarType::Int)))
        .collect();
    let refs: Vec<_> = compiled
        .refs
        .iter()
        .map(|r| match r {
            RefSlot::Form(f) => *f,
            other => panic!("owned fixture has an unexpected reference slot: {other:?}"),
        })
        .collect();
    let mut fields = vec![sub(b"EDID", z(name))];
    fields.extend(unit(0, &compiled.bytes, &vars, &refs));
    fields.push(sub(b"SCTX", src.into_bytes()));
    record(b"SCPT", 0, id, &fields)
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    files();
    let schema =
        Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/script/functions.json");
    let table = FunctionTable::load(&path).unwrap();
    let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let base = [
        tes4(),
        top(
            b"NPC_",
            &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]),
        ),
        top(
            b"SCPT",
            &[
                function(&table, 0x900, "OwnASet", true),
                function(&table, 0x901, "OwnAGet", false),
            ]
            .concat(),
        ),
    ]
    .concat();
    let header = record(
        b"TES4",
        0,
        0,
        &[
            sub(
                b"HEDR",
                [1.34f32.to_le_bytes().to_vec(), u32s(&[2, 0x902])].concat(),
            ),
            sub(b"MAST", z("OwnOrigin.ESM")),
            sub(b"DATA", vec![0; 8]),
        ],
    );
    let addon = [
        header,
        top(
            b"SCPT",
            &[
                function(&table, 0x0100_0900, "OwnBSet", true),
                function(&table, 0x0100_0901, "OwnBGet", false),
            ]
            .concat(),
        ),
    ]
    .concat();
    let plugins = vec![
        Plugin::parse(&base, "OwnOrigin.ESM".into()).unwrap(),
        Plugin::parse(&addon, "OwnAddon.EsP".into()).unwrap(),
    ];
    let lo = LoadOrder::new(&plugins).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &json, &refs);
    Fixture { data, forms, refs }
}

fn files() {
    // One immutable file tree and identical roots for every test in this process.
    static INIT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    let root = INIT.get_or_init(|| {
        let parent = std::env::var_os("NVF_ORIGIN_FIXTURE_TEMP").map(std::path::PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let root = parent.join(format!("nvf-runtime-origin-{}", std::process::id()));
        let scripts = root.join("NVSE/Plugins/Scripts");
        let udfs = root.join("NVSE/User_Defined_Functions/OwnOrigin");
        std::fs::create_dir_all(&scripts).unwrap();
        std::fs::create_dir_all(&udfs).unwrap();
        std::fs::write(scripts.join("gr_own_runtime_scope.txt"), "player.AuxVarSetFlt \"*NVFRuntimeScope\" 17\n").unwrap();
        std::fs::write(udfs.join("Read.gek"), "Begin Function {}\nSetFunctionValue (player.AuxVarGetFlt \"*NVFRuntimeScope\")\nEnd\n").unwrap();
        std::fs::write(udfs.join("Write.gek"), "Begin Function {}\nplayer.AuxVarSetFlt \"*NVFRuntimeScope\" 23\nSetFunctionValue (player.AuxVarGetFlt \"*NVFRuntimeScope\")\nEnd\n").unwrap();
        root
    });
    fnv_game::mod_files::set_roots(fnv_game::mod_files::Roots {
        read: vec![root.clone()],
        ..Default::default()
    });
}

fn game(f: &Fixture) -> Game<'_> {
    Game::new(
        &f.data,
        World::new(&f.forms, &f.refs, SimConfig::default()),
        GameConfig {
            strict: true,
            ..Default::default()
        },
    )
}
fn q(g: &mut Game, body: &str) -> XVal {
    let result = g
        .run_query(
            &format!("Begin Function {{}}\n{body}\nEnd\n"),
            RunCtx::default(),
        )
        .unwrap_or_else(|e| panic!("{body}: {e}"));
    clean(g);
    result
}
fn clean(g: &Game) {
    assert!(
        !g.events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ScriptError { .. })),
        "{:?}",
        g.events
    );
}
fn restart(g: &mut Game) {
    let runs = g.run_script_runner(Moment::Restart);
    assert_eq!(runs.len(), 1, "{runs:?}");
    assert!(runs[0].result.is_ok(), "{runs:?}");
    clean(g);
}

#[test]
fn runtime_origin_script_runner_auxvar_is_visible_to_query() {
    let f = fixture();
    let mut g = game(&f);
    restart(&mut g);
    assert_eq!(
        q(
            &mut g,
            "SetFunctionValue (player.AuxVarGetFlt \"*NVFRuntimeScope\")"
        ),
        XVal::Num(17.0)
    );
}

#[test]
fn runtime_origin_script_runner_auxvar_is_visible_to_compile_script() {
    let f = fixture();
    let mut g = game(&f);
    restart(&mut g);
    assert_eq!(
        q(
            &mut g,
            "SetFunctionValue (Call (CompileScript \"OwnOrigin\\Read.gek\"))"
        ),
        XVal::Num(17.0)
    );
}

#[test]
fn runtime_origin_query_console_and_compile_script_share_one_store() {
    let f = fixture();
    let mut g = game(&f);
    q(&mut g, "player.AuxVarSetFlt \"*NVFRuntimeScope\" 19");
    g.run_source(
        "player.AuxVarSetFlt \"*NVFRuntimeScope\" 21",
        RunCtx::default(),
    )
    .unwrap();
    assert_eq!(
        q(
            &mut g,
            "SetFunctionValue (player.AuxVarGetFlt \"*NVFRuntimeScope\")"
        ),
        XVal::Num(21.0)
    );
    assert_eq!(
        q(
            &mut g,
            "SetFunctionValue (Call (CompileScript \"OwnOrigin\\Read.gek\"))"
        ),
        XVal::Num(21.0)
    );
    assert_eq!(
        q(
            &mut g,
            "SetFunctionValue (Call (CompileScript \"OwnOrigin\\Write.gek\"))"
        ),
        XVal::Num(23.0)
    );
    assert_eq!(
        q(
            &mut g,
            "SetFunctionValue (player.AuxVarGetFlt \"*NVFRuntimeScope\")"
        ),
        XVal::Num(23.0)
    );
}

#[test]
fn runtime_origin_real_esm_esp_units_keep_private_plugin_stores() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(
        q(&mut g, "SetFunctionValue (Call OwnASet 31)"),
        XVal::Num(31.0)
    );
    assert_eq!(
        q(&mut g, "SetFunctionValue (Call OwnBSet 47)"),
        XVal::Num(47.0)
    );
    q(&mut g, "player.AuxVarSetFlt \"*OwnOriginPrivate\" 13");
    assert_eq!(
        q(&mut g, "SetFunctionValue (Call OwnAGet)"),
        XVal::Num(31.0)
    );
    assert_eq!(
        q(&mut g, "SetFunctionValue (Call OwnBGet)"),
        XVal::Num(47.0)
    );
    assert_eq!(
        q(
            &mut g,
            "SetFunctionValue (player.AuxVarGetFlt \"*OwnOriginPrivate\")"
        ),
        XVal::Num(13.0)
    );
    assert_eq!(
        q(
            &mut g,
            "SetFunctionValue (player.AuxVarGetFlt \"*_OwnOriginShared\")"
        ),
        XVal::Num(47.0)
    );
}
