//! JohnnyGuitar's NIF, particle and camera functions (#1007;
//! docs/spec-notes/ext-jg-nif-camera.md) on a synthetic plugin, called from scripts, with
//! the engine's part played by the test: the scene mirror's tree with what the engine
//! tells of each node, the camera's place, the hit point.

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::functions::camera::CameraView;
use fnv_game::game::{Game, GameConfig, RunCtx};
use fnv_game::nif_blocks::{NodeInfo, SceneEdit, SceneNode};
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x900;
const P: FormId = FormId::PLAYER_REF;

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let npcs = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player"))]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestRoom")), sub(b"DATA", vec![1])]);
    let bytes = [tes4(), top(b"NPC_", &npcs), top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &cell)))].concat();
    let plugins = vec![Plugin::parse(&bytes, "JgNifTest.esm".into()).unwrap()];
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

fn node(name: &str, parent: Option<usize>, translation: [f32; 3], info: NodeInfo) -> SceneNode {
    SceneNode { name: name.into(), parent, translation, rotation: [0.0; 3], scale: 1.0, flags: 0, material: None, info: Some(Box::new(info)) }
}

fn emitter() -> [f32; 21] {
    let mut e = [0.0; 21];
    e[1] = 1.0;
    e[14] = 3.0;
    e[20] = 50.0;
    e
}

/// The player at the origin with a tree: a root, a head 100 up, a switch node with three
/// children, a particle system and a shape with alpha and stencil properties.
fn tree() -> Vec<SceneNode> {
    let none = NodeInfo::default;
    vec![
        node("Scene Root", None, [0.0; 3], NodeInfo { bound: Some([0.0, 0.0, 60.0, 70.0]), ..none() }),
        node("Bip01 Head", Some(0), [0.0, 0.0, 100.0], none()),
        node("Switch", Some(0), [0.0; 3], none()),
        node("A", Some(2), [0.0; 3], none()),
        node("B", Some(2), [0.0; 3], none()),
        node("C", Some(2), [0.0; 3], none()),
        node("ParticleEmitter", Some(0), [0.0; 3], NodeInfo { emitter: Some(emitter()), ..none() }),
        node("Glass", Some(0), [0.0; 3], NodeInfo { alpha: Some([4845.0, 128.0]), stencil: Some([19840.0, 0.0, 4294967295.0]), ..none() }),
    ]
}

fn game(f: &Fixture) -> Game<'_> {
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.player_entered(Some(FormId(CELL)));
    g.scene.set_tree(P, tree());
    g
}

const VARS: &str = "float fX\nfloat fY\nfloat fZ\nfloat fOn\narray_var aR\n";

fn q(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("{VARS}Begin Function {{ }}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn n(g: &mut Game, expr: &str) -> f64 {
    let body = if expr.contains('\n') { expr.to_string() } else { format!("SetFunctionValue ({expr})") };
    match q(g, &body) {
        XVal::Num(x) => x,
        v => panic!("{expr}: {v:?}"),
    }
}

#[test]
fn distances_and_bounds() {
    let f = fixture();
    let mut g = game(&f);
    let at = g.ref_pos_rot(P).unwrap().0;
    let head = [at[0], at[1], at[2] + 100.0];
    // The head is 100 above the player's place.
    assert!((n(&mut g, &format!("PlayerREF.Get3DDistanceToNiNode \"Bip01 Head\" {} {} {}", head[0], head[1], head[2] + 30.0)) - 30.0).abs() < 1e-3);
    assert!((n(&mut g, "Get3DDistanceBetweenNiNodes PlayerREF PlayerREF \"Bip01 Head\" \"Scene Root\"") - 100.0).abs() < 1e-3);
    // No hit yet: 0; then 40 from the head.
    assert_eq!(n(&mut g, "PlayerREF.Get3DDistanceFromHitToNiNode \"Bip01 Head\""), 0.0);
    g.note_hit_point(P, [head[0] + 40.0, head[1], head[2]]);
    assert!((n(&mut g, "PlayerREF.Get3DDistanceFromHitToNiNode \"Bip01 Head\"") - 40.0).abs() < 1e-3);
    // The root's bound without a block.
    assert_eq!(n(&mut g, "let aR := PlayerREF.GetNiBound\nSetFunctionValue aR[2] * 1000 + aR[3]"), 60_070.0);
    // A block it does not have: the root's.
    assert_eq!(n(&mut g, "let aR := PlayerREF.GetNiBound \"NVFNoSuchBlock\"\nSetFunctionValue aR[2] * 1000 + aR[3]"), 60_070.0);
}

#[test]
fn particles_properties_and_switches() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(n(&mut g, "PlayerREF.GetNiPSysModifierValue \"ParticleEmitter\" 14"), 3.0);
    assert_eq!(n(&mut g, "PlayerREF.GetParticleEmitterSpawnRate \"ParticleEmitter\""), 50.0);
    assert_eq!(n(&mut g, "PlayerREF.GetNiPSysModifierValue \"Glass\" 14"), 0.0);
    g.scene.take_edits();
    assert_eq!(n(&mut g, "PlayerREF.SetParticleEmitterSpawnRate \"ParticleEmitter\" 1000"), 1.0);
    assert_eq!(n(&mut g, "PlayerREF.SetNiPSysModifierValue \"ParticleEmitter\" 14 5"), 1.0);
    assert_eq!(n(&mut g, "PlayerREF.SetNiPSysModifierValue \"Nothing\" 14 5"), 0.0);
    assert_eq!(n(&mut g, "PlayerREF.GetParticleEmitterSpawnRate \"ParticleEmitter\""), 1000.0);
    let edits = g.scene.take_edits();
    assert!(edits.contains(&SceneEdit::Particle { r: P, node: "ParticleEmitter".into(), id: 20, v: 1000.0 }));
    // The properties: read, set, kept over the next tree the engine tells.
    assert_eq!(n(&mut g, "PlayerREF.GetAlphaPropertyValue \"Glass\" 1"), 128.0);
    assert_eq!(n(&mut g, "PlayerREF.SetAlphaPropertyValue \"Glass\" 1 64"), 1.0);
    assert_eq!(n(&mut g, "PlayerREF.SetStencilPropertyValue \"Glass\" 1 3"), 1.0);
    g.scene.set_tree(P, tree());
    assert_eq!(n(&mut g, "PlayerREF.GetAlphaPropertyValue \"Glass\" 1") * 10.0 + n(&mut g, "PlayerREF.GetStencilPropertyValue \"Glass\" 1"), 643.0);
    // The switch shows its second child alone.
    q(&mut g, "PlayerREF.SetSwitchNodeIndex \"Switch\" 1");
    let flags: Vec<(String, usize, u32)> = g.scene.take_edits().into_iter().filter_map(|e| match e {
        SceneEdit::ChildFlags { node, child, flags, .. } => Some((node, child, flags & 1)),
        _ => None,
    }).collect();
    assert_eq!(flags, vec![("Switch".into(), 0, 1), ("Switch".into(), 1, 0), ("Switch".into(), 2, 1)]);
    // The 3D drawn again for the model flags, not for lights alone.
    g.take_appearance_changes();
    q(&mut g, "PlayerREF.Update3DAlt 32");
    assert!(g.take_appearance_changes().is_empty());
    q(&mut g, "PlayerREF.Update3DAlt 1");
    assert_eq!(g.take_appearance_changes().len(), 1);
    assert_eq!(n(&mut g, "PlayerREF.UpdateScenegraph 0"), 1.0);
    assert_eq!(n(&mut g, "PlayerREF.Update3DAlt 32"), 1.0);
    // Standing (nothing told): Idle (0) in slot 0, nothing in slot 4.
    assert_eq!(n(&mut g, "PlayerREF.GetSequenceAnimGroup 0"), 0.0);
    assert_eq!(n(&mut g, "PlayerREF.GetSequenceAnimGroup 4"), -1.0);
}

#[test]
fn a_switch_addresses_its_children_by_place() {
    // Children unnamed, or named like a node elsewhere in the model (#2017): each is
    // hidden or shown by its place under the switch, and no other node changes.
    let f = fixture();
    let mut g = game(&f);
    let none = NodeInfo::default;
    g.scene.set_tree(P, vec![
        node("Scene Root", None, [0.0; 3], none()),
        node("Lamp", Some(0), [0.0; 3], none()),
        node("Switch", Some(0), [0.0; 3], none()),
        node("", Some(2), [0.0; 3], none()),
        node("Lamp", Some(2), [0.0; 3], none()),
        node("", Some(2), [0.0; 3], none()),
    ]);
    for (k, want) in [(0, [0, 1, 1]), (1, [1, 0, 1]), (2, [1, 1, 0]), (3, [1, 1, 1])] {
        q(&mut g, &format!("PlayerREF.SetSwitchNodeIndex \"Switch\" {k}"));
        let edits: Vec<(usize, u32)> = g.scene.take_edits().into_iter().map(|e| match e {
            SceneEdit::ChildFlags { node, child, flags, .. } if node == "Switch" => (child, flags & 1),
            other => panic!("index {k}: {other:?}"),
        }).collect();
        assert_eq!(edits, (0..3).map(|c| (c, want[c])).collect::<Vec<_>>(), "index {k}");
        // The Lamp beside the switch (found first by name) is left alone.
        assert_eq!(g.scene.node_now(P, "Lamp").map(|n| n.flags & 1), Some(0), "index {k}");
    }
}

#[test]
fn the_camera() {
    let f = fixture();
    let mut g = game(&f);
    // No engine yet: zeros.
    assert_eq!(n(&mut g, "GetCameraTranslation fX fY fZ 0\nSetFunctionValue fX + fY + fZ"), 0.0);
    let at = g.ref_pos_rot(P).unwrap().0;
    // The camera 100 behind the player, facing north with a 90° view, square.
    g.ui.camera.view = Some(CameraView { pos: [at[0], at[1] - 100.0, at[2]], heading: 0.0, pitch: 0.0, roll: 0.0, fov_y: std::f32::consts::FRAC_PI_2, aspect: 1.0, first_person: false });
    assert_eq!(n(&mut g, "GetCameraTranslation fX fY fZ 1\nSetFunctionValue fY"), -100.0);
    // The player's place is at the screen's centre, 100 ahead.
    assert_eq!(n(&mut g, "let fOn := WorldToScreen fX fY fZ 0 0 0 0 PlayerREF\nSetFunctionValue fOn * 1000 + fX * 100 + fY * 10 + fZ / 100"), 1000.0 + 50.0 + 5.0 + 1.0);
    // The overrides for the engine to put on the third-person camera.
    q(&mut g, "SetCameraTranslate 1 10 -50 20\nSetCameraRotate 1 3 90");
    assert_eq!(g.ui.camera.translate_override, Some([10.0, -50.0, 20.0]));
    assert_eq!(g.ui.camera.rotate_override, [None, None, Some(90.0)]);
    q(&mut g, "SetCameraTranslate 0 0 0 0\nSetCameraRotate 0 3 0");
    assert_eq!((g.ui.camera.translate_override, g.ui.camera.rotate_override), (None, [None; 3]));
}

#[test]
fn nif_translation_world_flag_composes_reference_and_parent_instead_of_returning_local() {
    let f = fixture();
    let mut g = game(&f);
    q(&mut g,"PlayerREF.SetPos X 100\nPlayerREF.SetPos Y -200\nPlayerREF.SetPos Z 300\nPlayerREF.SetAngle Z 90");
    let mut root = node("OwnedRoot",None,[10.0,20.0,30.0],NodeInfo::default());
    root.scale=2.0;
    g.scene.set_tree(P,vec![root,node("OwnedHand",Some(0),[1.0,2.0,3.0],NodeInfo::default())]);
    let sum = |world|format!("let aR := PlayerREF.GetNifBlockTranslation \"OwnedHand\" {world}\nSetFunctionValue aR[0] * 10000 + aR[1] * 100 + aR[2]");
    assert_eq!(n(&mut g,&sum(0)),10203.0);
    assert!((n(&mut g,&sum(1))-(76.0*10000.0-188.0*100.0+336.0)).abs()<0.5,"getWorld1 must compose the owning reference and parent scale");
    assert_eq!(n(&mut g,"let aR := PlayerREF.GetNifBlockTranslation \"OwnedHand\"\nSetFunctionValue aR[0]*10000+aR[1]*100+aR[2]"),10203.0);
    assert_eq!(n(&mut g,"ar_Size (PlayerREF.GetNifBlockTranslation \"MissingOwnedNode\" 1)"),-1.0);
    let axis=fnv_game::nvse::call(&mut g,&Default::default(),"getnifblocktranslation",Some(P),&[XVal::Str("OwnedHand".into()),XVal::Str("Z".into()),XVal::Num(1.0)]).unwrap().unwrap();
    assert_eq!(axis,XVal::Num(336.0));
}
