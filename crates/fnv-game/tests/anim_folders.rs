//! kNVSE's AnimGroupOverride folders (#1461, docs/spec-notes/knvse-anim-folders.md): a
//! made-up Data folder with a weapon's override, a conditioned one and one for every
//! actor, read through a synthetic plugin (no game data). Its own test binary: the
//! Data roots are global to the process.

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const WEAPON: u32 = 0x800;
const OTHER_GUN: u32 = 0x801;
const FLAG: u32 = 0x802;
const ROOM: u32 = 0x803;

fn plugin_bytes() -> Vec<u8> {
    let npc_data = [50i32.to_le_bytes().to_vec(), vec![5; 7]].concat();
    let player = record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"DATA", npc_data)]);
    let weap_data = [&10i32.to_le_bytes()[..], &100i32.to_le_bytes(), &0.5f32.to_le_bytes(), &10i16.to_le_bytes(), &[12u8]].concat();
    let weaps = [
        record(b"WEAP", 0, WEAPON, &[sub(b"EDID", z("TestGun")), sub(b"DATA", weap_data.clone())]),
        record(b"WEAP", 0, OTHER_GUN, &[sub(b"EDID", z("OtherGun")), sub(b"DATA", weap_data)]),
    ]
    .concat();
    let glob = record(b"GLOB", 0, FLAG, &[sub(b"EDID", z("TestFlag")), sub(b"FNAM", vec![b'f']), sub(b"FLTV", 0f32.to_le_bytes().to_vec())]);
    let room = record(b"CELL", 0, ROOM, &[sub(b"EDID", z("TestRoom")), sub(b"DATA", vec![1])]);
    [
        tes4(),
        top(b"GLOB", &glob),
        top(b"NPC_", &player),
        top(b"WEAP", &weaps),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &room))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Folders.esm".into()).unwrap()];
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

/// The Data folder: `meshes\AnimGroupOverride` with the JSON files and their folders,
/// spelled in mixed case as mods spell them, plus an empty, a broken and a JSON file
/// naming a folder that is not there (all left out).
fn files() {
    use fnv_game::mod_files::{set_roots, Roots};
    let dir = std::env::temp_dir().join(format!("nvf-anim-folders-{}", std::process::id()));
    let ago = dir.join("Meshes/AnimGroupOverride");
    for f in ["Gun/_male/2hrAttack3.kf", "Gun/_1stPerson/2hrAttack3.kf", "Gun/_1stPerson/Mod1/2hrReloadA.kf", "Gun/_1stPerson/chances.txt", "Gun/Flag/_1stperson/2hrAttack3.kf", "Everyone/_Male/Locomotion/mtidle.kf"] {
        let p = ago.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, b"kf").unwrap();
    }
    std::fs::write(
        ago.join("TestGun.json"),
        r#"[
            {"mod": "Folders.esm", "form": "800", "folder": "Gun"},
            {"mod": "Folders.esm", "form": ["801", "800"], "folder": "Gun\\Flag", "condition": "TestFlag == 1", "priority": 1},
            {"mod": "NotLoaded.esp", "form": "800", "folder": "Gun\\Flag"},
            {"mod": "Folders.esm", "form": "800", "folder": "Missing"},
            {"mod": "Folders.esm", "folder": "Gun"}
        ]"#,
    )
    .unwrap();
    std::fs::write(ago.join("everyone.json"), r#"{"folder": "Everyone", "condition": "this == Player"}"#).unwrap();
    std::fs::write(ago.join("Empty.json"), "").unwrap();
    std::fs::write(ago.join("Broken.json"), "[{").unwrap();
    set_roots(Roots { read: vec![dir], ..Roots::default() });
}

fn q(g: &mut Game, body: &str) {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), Default::default()).unwrap_or_else(|e| panic!("{body}: {e}"));
}

#[test]
fn folder_overrides_follow_the_weapon_and_the_condition() {
    files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.player_entered(Some(FormId(ROOM)));
    g.tick();
    // Three entries kept, the files read in name order without regard to case:
    // everyone's, the gun's and the conditioned one.
    let folders: Vec<&str> = g.anim_folders.entries.iter().map(|e| e.folder.as_str()).collect();
    assert_eq!(folders, ["Everyone", "Gun", "Gun\\Flag"]);
    // The Mod1 file and the text file are not among the gun's.
    assert_eq!(g.anim_folders.entries[1].first, ["AnimGroupOverride\\Gun\\_1stPerson\\2hrAttack3.kf"]);
    // Without the gun: only everyone's entry (its condition holds on the player).
    g.refresh_anim_folders();
    assert_eq!(g.anim_overrides(FormId::PLAYER_REF, false), ["AnimGroupOverride\\Everyone\\_Male\\Locomotion\\mtidle.kf"]);
    assert!(g.anim_overrides(FormId::PLAYER_REF, true).is_empty());
    // With the gun: its files, both views.
    q(&mut g, "player.AddItem TestGun 1\nplayer.EquipItem TestGun");
    g.refresh_anim_folders();
    assert_eq!(g.anim_overrides(FormId::PLAYER_REF, true), ["AnimGroupOverride\\Gun\\_1stPerson\\2hrAttack3.kf"]);
    assert_eq!(g.anim_overrides(FormId::PLAYER_REF, false), ["AnimGroupOverride\\Everyone\\_Male\\Locomotion\\mtidle.kf", "AnimGroupOverride\\Gun\\_male\\2hrAttack3.kf"]);
    // The conditioned folder only while its condition holds, after the plain one (it
    // wins the group).
    q(&mut g, "set TestFlag to 1");
    g.refresh_anim_folders();
    assert_eq!(g.anim_overrides(FormId::PLAYER_REF, true), ["AnimGroupOverride\\Gun\\_1stPerson\\2hrAttack3.kf", "AnimGroupOverride\\Gun\\Flag\\_1stperson\\2hrAttack3.kf"]);
    q(&mut g, "set TestFlag to 0");
    g.refresh_anim_folders();
    assert_eq!(g.anim_overrides(FormId::PLAYER_REF, true).len(), 1);
    // A script's path comes after the folders' (it wins).
    q(&mut g, "player.SetActorAnimationPath 1 1 \"AnimGroupOverride\\Gun\\Flag\\_1stperson\\2hrAttack3.kf\"");
    assert_eq!(g.anim_overrides(FormId::PLAYER_REF, true).last().map(String::as_str), Some("AnimGroupOverride\\Gun\\Flag\\_1stperson\\2hrAttack3.kf"));
    // CopyAnimationsToForm takes the folders' entries along, conditions and all.
    q(&mut g, "CopyAnimationsToForm TestGun OtherGun");
    assert!(g.anim_folders.entries.iter().filter(|e| e.forms.contains(&FormId(WEAPON))).all(|e| e.forms.contains(&FormId(OTHER_GUN))));
}

/// SetActorAnimationPath takes a folder (its KF files) and a condition function: the
/// path applies only while the condition holds [documented: SetActorAnimationPath].
#[test]
fn script_paths_take_folders_and_conditions() {
    files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.player_entered(Some(FormId(ROOM)));
    g.tick();
    let gun = |v: Vec<String>| v.into_iter().map(|p| p.replace(char::from(92), "/")).filter(|p| p.contains("Gun")).collect::<Vec<_>>();
    // A folder: its KF files, not the text file or the subfolder's.
    q(&mut g, "player.SetActorAnimationPath 1 1 \"AnimGroupOverride/Gun/_1stPerson\"");
    assert_eq!(gun(g.anim_overrides(FormId::PLAYER_REF, true)), ["AnimGroupOverride/Gun/_1stPerson/2hrAttack3.kf"]);
    // A conditional folder: out while TestFlag is 0, in once it is 1.
    q(&mut g, "player.SetActorAnimationPath 0 1 \"AnimGroupOverride/Gun/_male\" 1 ({} => TestFlag == 1)");
    assert!(gun(g.anim_overrides(FormId::PLAYER_REF, false)).is_empty(), "the plain list leaves conditional paths out");
    assert!(gun(g.anim_overrides_now(FormId::PLAYER_REF, false)).is_empty());
    q(&mut g, "set TestFlag to 1");
    assert_eq!(gun(g.anim_overrides_now(FormId::PLAYER_REF, false)), ["AnimGroupOverride/Gun/_male/2hrAttack3.kf"]);
    // Disabled: gone.
    q(&mut g, "player.SetActorAnimationPath 0 0 \"AnimGroupOverride/Gun/_male\"");
    assert!(gun(g.anim_overrides_now(FormId::PLAYER_REF, false)).is_empty());
}

/// SetWeaponAnimationPath's files apply to whoever holds the weapon, a folder giving its
/// KF files [documented: SetWeaponAnimationPath; hypothesis: the holder].
#[test]
fn weapon_paths_follow_the_weapon_in_hand() {
    files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.player_entered(Some(FormId(ROOM)));
    g.tick();
    let gun = |v: Vec<String>| v.into_iter().map(|p| p.replace(char::from(92), "/")).filter(|p| p.contains("Gun/_male")).collect::<Vec<_>>();
    q(&mut g, "SetWeaponAnimationPath OtherGun 0 1 \"AnimGroupOverride/Gun/_male\"");
    assert!(gun(g.anim_overrides(FormId::PLAYER_REF, false)).is_empty(), "not held");
    q(&mut g, "player.AddItem OtherGun 1\nplayer.EquipItem OtherGun");
    assert_eq!(gun(g.anim_overrides(FormId::PLAYER_REF, false)), ["AnimGroupOverride/Gun/_male/2hrAttack3.kf"]);
    assert_eq!(gun(g.anim_overrides_now(FormId::PLAYER_REF, false)), ["AnimGroupOverride/Gun/_male/2hrAttack3.kf"]);
    assert!(gun(g.anim_overrides(FormId::PLAYER_REF, true)).is_empty(), "the third-person path only");
}

/// PlayAnimationPath's sequences finish by themselves; ActivateAnim's stay until a script
/// takes them back [documented: ActivateAnim, "adds a sequence to the active ones"]. Both
/// are requests in `AnimPaths::sequences`, which the renderer layers on the actor.
#[test]
fn activate_anim_sequences_stay_until_taken_back() {
    files();
    let f = fixture();
    let world = World::new(&f.forms, &f.refs, SimConfig::default());
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.player_entered(Some(FormId(ROOM)));
    q(&mut g, "player.PlayAnimationPath \"AnimGroupOverride/Gun/_male/2hrAttack3.kf\"
player.ActivateAnim \"AnimGroupOverride/Gun/_1stPerson/2hrAttack3.kf\"");
    let played = g.anim_paths.played.get(&FormId::PLAYER_REF).cloned().unwrap_or_default();
    assert_eq!(played.len(), 2);
    let mut kept: Vec<(String, bool)> = g.anim_paths.sequences.iter()
        .filter(|((a, _, _), _)| *a == FormId::PLAYER_REF)
        .map(|((_, _, p), s)| (p.replace(char::from(92), "/"), s.auto_finish))
        .collect();
    kept.sort();
    assert_eq!(kept, [("animgroupoverride/gun/_1stperson/2hrattack3.kf".to_string(), false), ("animgroupoverride/gun/_male/2hrattack3.kf".to_string(), true)]);
}
