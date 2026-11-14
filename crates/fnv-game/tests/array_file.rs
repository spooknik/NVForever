//! JIP's ReadArrayFromFile on a file written the way its page shows and Food Scraps'
//! config files do (#2275): tabs between values, `$` strings, `@File:XXXXXX` forms, `;`
//! comments. A synthetic plugin and a made-up file (no game data).

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

#[test]
fn read_array_from_file_takes_strings_forms_and_skips_comments() {
    use fnv_game::mod_files::{set_roots, Roots};
    let dir = std::env::temp_dir().join(format!("nvf-array-file-{}", std::process::id()));
    let root = dir.join("w").join("GameRoot");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(dir.join("data")).unwrap();
    std::fs::write(
        root.join("scraps.txt"),
        "; [form]\t[item]\t[chance]\t\t; Item Name\r\n\r\n; ====\r\n; @AF.esm:000801\t\t$milk\t\t40\r\n@AF.esm:000800\t$tincan\t\t; Can\r\n@af.esm:000801\t\t@AF.esm:000800\t50\t; Other\r\n@NotLoaded.esp:000123\t$soda\t7\r\n",
    )
    .unwrap();
    set_roots(Roots { read: vec![dir.join("data")], write: Some(dir.join("w")), ..Roots::default() });

    let glob = |fid: u32, edid: &str| record(b"GLOB", 0, fid, &[sub(b"EDID", z(edid)), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())]);
    let bytes = [tes4(), top(b"GLOB", &[glob(0x800, "AFOne"), glob(0x801, "AFTwo")].concat())].concat();
    let plugins = vec![Plugin::parse(&bytes, "AF.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    let (forms, refs) = (FormStore::build(&lo, &schema), RefTable::build(&lo, &schema));
    let data = GameData::build(&lo, &schema, FunctionTable::load(&p).unwrap(), &j, &refs);
    let mut g = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig::default());
    let ask = |g: &mut Game, expr: &str| {
        g.run_query(&format!("array_var a\nBegin Function {{ }}\nlet a := ReadArrayFromFile \"scraps.txt\"\nSetFunctionValue ({expr})\nEnd\n"), Default::default()).unwrap()
    };
    // Two rows; columns first: form, item, chance.
    assert_eq!(ask(&mut g, "Ar_Size a[0]"), XVal::Num(3.0));
    assert_eq!(ask(&mut g, "a[0][0]"), XVal::Ref(0x800));
    assert_eq!(ask(&mut g, "a[0][1]"), XVal::Ref(0x801));
    assert_eq!(ask(&mut g, "a[1][0]"), XVal::Str("tincan".into()));
    assert_eq!(ask(&mut g, "a[1][1]"), XVal::Ref(0x800));
    assert_eq!(ask(&mut g, "a[2][0]"), XVal::Num(50.0));
    // A form of a file not loaded keeps its column, as the empty form.
    assert_eq!(ask(&mut g, "a[0][2]"), XVal::Ref(0));
    assert_eq!(ask(&mut g, "a[1][2]"), XVal::Str("soda".into()));
}
