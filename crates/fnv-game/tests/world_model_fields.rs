//! Independently authored record data for the GetModelPath world-object boundary.
use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::{
    GameData,
    game::{Game, GameConfig, RunCtx},
};
use fnv_script::{FunctionTable, vm::XVal};
use fnv_sim::{FormStore, RefTable, SimConfig, World};

const MODEL: &str = "Owned\\Original Model.NIF";
const TYPES: [([u8; 4], &str); 6] = [
    (*b"STAT", "OwnStaticModel"),
    (*b"MSTT", "OwnMovableModel"),
    (*b"FURN", "OwnFurnitureModel"),
    (*b"DOOR", "OwnDoorModel"),
    (*b"TACT", "OwnTalkingModel"),
    (*b"ACTI", "OwnActivatorModel"),
];

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let mut bytes = tes4();
    bytes.extend(top(
        b"NPC_",
        &record(b"NPC_", 0, 7, &[sub(b"EDID", z("Player"))]),
    ));
    for (index, (sig, name)) in TYPES.iter().enumerate() {
        bytes.extend(top(
            sig,
            &record(
                sig,
                0,
                0xD00 + index as u32,
                &[sub(b"EDID", z(name)), sub(b"MODL", z(MODEL))],
            ),
        ));
    }
    bytes.extend(top(
        b"STAT",
        &record(b"STAT", 0, 0xD10, &[sub(b"EDID", z("OwnNoModel"))]),
    ));
    let plugins = vec![Plugin::parse(&bytes, "OwnedModels.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
    let schema = Schema::load_dir(root.join("fnv")).unwrap();
    let functions = root.join("script/functions.json");
    let json = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
    let forms = FormStore::build(&lo, &schema);
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(
        &lo,
        &schema,
        FunctionTable::load(&functions).unwrap(),
        &json,
        &refs,
    );
    Fixture { data, forms, refs }
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
    g.run_query(
        &format!("Begin Function {{}}\n{body}\nEnd"),
        RunCtx::default(),
    )
    .unwrap()
}

#[test]
fn authored_world_object_models_are_readable_without_path_rewriting() {
    let f = fixture();
    let mut g = game(&f);
    for (_, name) in TYPES {
        assert_eq!(
            q(&mut g, &format!("SetFunctionValue (GetModelPath {name})")),
            XVal::Str(MODEL.into()),
            "authored MODL for {name}"
        );
    }
    assert_eq!(
        q(&mut g, "SetFunctionValue (GetModelPath OwnNoModel)"),
        XVal::Str(String::new())
    );
    assert_eq!(
        q(&mut g, "SetFunctionValue (GetModelPath 0)"),
        XVal::Str(String::new())
    );
}

#[test]
fn clone_model_edits_leave_the_original_and_other_games_independent() {
    let f = fixture();
    let mut g = game(&f);
    let XVal::Ref(clone) = q(
        &mut g,
        "ref ownClone\nlet ownClone := TempCloneForm OwnStaticModel 1\nSetFunctionValue ownClone",
    ) else {
        panic!("a clone reference is required")
    };
    assert_eq!(
        q(
            &mut g,
            &format!("SetFunctionValue (GetModelPath {clone:#010X})")
        ),
        XVal::Str(MODEL.into())
    );
    q(
        &mut g,
        &format!(r#"SetModelPath "owned\clone.nif" {clone:#010X}"#),
    );
    assert_eq!(
        q(
            &mut g,
            &format!("SetFunctionValue (GetModelPath {clone:#010X})")
        ),
        XVal::Str("owned\\clone.nif".into())
    );
    assert_eq!(
        q(&mut g, "SetFunctionValue (GetModelPath OwnStaticModel)"),
        XVal::Str(MODEL.into())
    );
    let mut next = game(&f);
    assert_eq!(
        q(&mut next, "SetFunctionValue (GetModelPath OwnStaticModel)"),
        XVal::Str(MODEL.into())
    );
}

#[test]
fn a_placed_reference_reads_its_authored_base_model() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(
        q(
            &mut g,
            "ref ownPlaced\nlet ownPlaced := player.PlaceAtMe OwnStaticModel\nSetFunctionValue (ownPlaced.GetModelPath)"
        ),
        XVal::Str(MODEL.into())
    );
}
