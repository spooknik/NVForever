//! ITR NVSE batch 2 (`fnv_game::nvse_itr2`, docs/spec-notes/ext-itr.md §7-12) through
//! compiled scripts, on a synthetic plugin (no game data, made-up records): dialogue
//! rules, speech and gestures, fake hits, combat targets, ammo, aid items, resurrection,
//! paths and cover through a lent navmesh answerer, ground through a lent ray caster,
//! doors, references by distance, perk eligibility, recipes and the menus.

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::game::{Game, GameConfig};
use fnv_game::nav_ask::NavAsk;
use fnv_game::GameData;
use fnv_script::vm::XVal;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const RACE: u32 = 0x810;
const RACE2: u32 = 0x811;
const NPC: u32 = 0x820;
const CREA: u32 = 0x821;
const WEAP: u32 = 0x830;
const AMMO: u32 = 0x831;
const ALCH: u32 = 0x832;
const MISC: u32 = 0x833;
const FLST: u32 = 0x834;
const DOOR: u32 = 0x835;
const TOPIC: u32 = 0x840;
const INFO_A: u32 = 0x841;
const INFO_B: u32 = 0x842;
const PERK_GUNS: u32 = 0x850;
const PERK_FREE: u32 = 0x851;
const PERK_TRAIT: u32 = 0x852;
const RCCT: u32 = 0x860;
const RCPE: u32 = 0x861;
const WRLD: u32 = 0x870;
const CELL_A: u32 = 0x200;
const CELL_B: u32 = 0x201;
const EXT: u32 = 0x202;
const NPC_REF: u32 = 0x300;
const CREA_REF: u32 = 0x301;
const DOOR_A: u32 = 0x310;
const DOOR_B_IN: u32 = 0x311;
const DOOR_B_OUT: u32 = 0x312;
const DOOR_EXT: u32 = 0x313;

/// Guns (actor value 41).
const GUNS: u32 = 41;

fn f32b(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn placed(sig: &[u8; 4], fid: u32, edid: &str, base: u32, pos: [f32; 3]) -> Vec<u8> {
    let data = [f32b(pos[0]), f32b(pos[1]), f32b(pos[2]), vec![0; 12]].concat();
    record(sig, 0, fid, &[sub(b"EDID", z(edid)), sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", data)])
}

fn door(fid: u32, edid: &str, to: u32) -> Vec<u8> {
    let xtel = [to.to_le_bytes().to_vec(), [0f32; 6].iter().flat_map(|f| f.to_le_bytes()).collect(), 0u32.to_le_bytes().to_vec()].concat();
    record(b"REFR", 0, fid, &[sub(b"EDID", z(edid)), sub(b"NAME", DOOR.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24]), sub(b"XTEL", xtel)])
}

/// A condition: type byte (comparison in the top 3 bits), value, function, parameter.
fn ctda(ty: u8, value: f32, function: u16, p1: u32) -> fnv_esm::raw::Subrecord {
    let mut d = vec![ty, 0, 0, 0];
    d.extend(value.to_le_bytes());
    d.extend(function.to_le_bytes());
    d.extend([0, 0]);
    d.extend(p1.to_le_bytes());
    d.extend([0; 12]);
    sub(b"CTDA", d)
}

const GE: u8 = 0x60;
const EQ: u8 = 0x00;

fn plugin_bytes() -> Vec<u8> {
    let race = [
        record(b"RACE", 0, RACE, &[sub(b"EDID", z("TestRace")), sub(b"DATA", [vec![0; 16], vec![0; 16], u32s(&[0])].concat())]),
        record(b"RACE", 0, RACE2, &[sub(b"EDID", z("OtherRace")), sub(b"DATA", [vec![0; 16], vec![0; 16], u32s(&[0])].concat())]),
    ]
    .concat();
    let npc_data = [50i32.to_le_bytes().to_vec(), vec![5; 7]].concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), sub(b"DATA", npc_data.clone())]),
        record(b"NPC_", 0, NPC, &[sub(b"EDID", z("TestNpc")), sub(b"FULL", z("Tester")), sub(b"RNAM", RACE.to_le_bytes().to_vec()), sub(b"DATA", npc_data)]),
    ]
    .concat();
    let crea = record(b"CREA", 0, CREA, &[sub(b"EDID", z("TestBeast")), sub(b"DATA", [vec![0u8, 40, 0, 0], vec![0; 6], vec![0; 7]].concat())]);
    // WEAP DATA: value, health, weight, damage 7, clip 6; NAM0 the ammo.
    let weap_data = [10i32.to_le_bytes().to_vec(), 100i32.to_le_bytes().to_vec(), f32b(2.0), 7i16.to_le_bytes().to_vec(), vec![6]].concat();
    let weap = record(b"WEAP", 0, WEAP, &[sub(b"EDID", z("Gun")), sub(b"NAM0", AMMO.to_le_bytes().to_vec()), sub(b"DATA", weap_data)]);
    let ammo = record(b"AMMO", 0, AMMO, &[sub(b"EDID", z("Bullets"))]);
    let enit = [u32s(&[5]), vec![0; 4], u32s(&[0]), f32b(0.0), u32s(&[0])].concat();
    let alch = record(b"ALCH", 0, ALCH, &[sub(b"EDID", z("TestChem")), sub(b"DATA", f32b(0.5)), sub(b"ENIT", enit)]);
    let misc = record(b"MISC", 0, MISC, &[sub(b"EDID", z("Scrap")), sub(b"DATA", [1i32.to_le_bytes().to_vec(), f32b(1.0)].concat())]);
    let flst = record(b"FLST", 0, FLST, &[sub(b"EDID", z("GunList")), sub(b"LNAM", WEAP.to_le_bytes().to_vec())]);
    let doors = record(b"DOOR", 0, DOOR, &[sub(b"EDID", z("TestDoor"))]);
    // Two lines of one topic, neither saying its conditions pass for anybody.
    let info = |id: u32, edid: &str| record(b"INFO", 0, id, &[sub(b"EDID", z(edid)), sub(b"DATA", vec![0, 0, 0, 0]), ctda(EQ, 1.0, 72, 0x7)]);
    let dial = [record(b"DIAL", 0, TOPIC, &[sub(b"EDID", z("TestTopic")), sub(b"DATA", vec![0, 0])]), group(TOPIC.to_le_bytes(), 7, &[info(INFO_A, "TestInfoA"), info(INFO_B, "TestInfoB")].concat())].concat();
    // Perks: one at least level 2 needing Guns 45, whose damage entry filters on the gun
    // list; one of least level 0 without conditions; a trait.
    let perk = |fid: u32, edid: &str, data: Vec<u8>, conds: Vec<fnv_esm::raw::Subrecord>, entry: bool| {
        let mut s = vec![sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), sub(b"DATA", data)];
        s.extend(conds);
        if entry {
            s.push(sub(b"PRKE", vec![2, 0, 0]));
            s.push(sub(b"DATA", vec![0, 3, 1]));
            s.push(sub(b"PRKC", vec![1]));
            s.push(ctda(EQ, 1.0, 372, FLST));
            s.push(sub(b"EPFT", vec![1]));
            s.push(sub(b"EPFD", 1.25f32.to_le_bytes().to_vec()));
            s.push(sub(b"PRKF", vec![]));
        }
        record(b"PERK", 0, fid, &s)
    };
    let perks = [
        perk(PERK_GUNS, "GunPerk", vec![0, 2, 1, 1, 0], vec![ctda(GE, 45.0, 14, GUNS)], true),
        perk(PERK_FREE, "FreePerk", vec![0, 0, 1, 1, 0], vec![], false),
        perk(PERK_TRAIT, "TraitPerk", vec![1, 1, 1, 1, 0], vec![], false),
    ]
    .concat();
    let rcct = record(b"RCCT", 0, RCCT, &[sub(b"EDID", z("Bench")), sub(b"FULL", z("Bench")), sub(b"DATA", vec![0])]);
    let rcpe = record(
        b"RCPE",
        0,
        RCPE,
        &[sub(b"EDID", z("ScrapRecipe")), sub(b"DATA", [(-1i32).to_le_bytes().to_vec(), u32s(&[0, RCCT, 0])].concat()), sub(b"RCIL", MISC.to_le_bytes().to_vec()), sub(b"RCQY", u32s(&[2])), sub(b"RCOD", AMMO.to_le_bytes().to_vec()), sub(b"RCQY", u32s(&[1]))],
    );
    let cell = |fid: u32, edid: &str, refs: Vec<u8>| [record(b"CELL", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", vec![1])]), group(fid.to_le_bytes(), 6, &group(fid.to_le_bytes(), 9, &refs))].concat();
    let cell_a = cell(CELL_A, "CellA", [placed(b"ACHR", NPC_REF, "TestNpcRef", NPC, [100.0, 0.0, 0.0]), placed(b"ACRE", CREA_REF, "TestBeastRef", CREA, [50.0, 0.0, 0.0]), door(DOOR_A, "DoorA", DOOR_B_IN)].concat());
    let cell_b = cell(CELL_B, "CellB", [door(DOOR_B_IN, "DoorBIn", DOOR_A), door(DOOR_B_OUT, "DoorBOut", DOOR_EXT)].concat());
    let wrld = record(b"WRLD", 0, WRLD, &[sub(b"EDID", z("TestWorld"))]);
    let ext = record(b"CELL", 0, EXT, &[sub(b"EDID", z("TestExterior")), sub(b"XCLC", [0i32.to_le_bytes(), 0i32.to_le_bytes()].concat())]);
    let ext_refs = group(EXT.to_le_bytes(), 6, &group(EXT.to_le_bytes(), 9, &door(DOOR_EXT, "DoorExt", DOOR_B_OUT)));
    let world_children = group(WRLD.to_le_bytes(), 1, &group(0i32.to_le_bytes(), 4, &group(0i32.to_le_bytes(), 5, &[ext, ext_refs].concat())));
    [
        tes4(),
        top(b"RACE", &race),
        top(b"NPC_", &npcs),
        top(b"CREA", &crea),
        top(b"WEAP", &weap),
        top(b"AMMO", &ammo),
        top(b"ALCH", &alch),
        top(b"MISC", &misc),
        top(b"FLST", &flst),
        top(b"DOOR", &doors),
        top(b"DIAL", &dial),
        top(b"PERK", &perks),
        top(b"RCCT", &rcct),
        top(b"RCPE", &rcpe),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell_a, cell_b].concat()))),
        top(b"WRLD", &[wrld, world_children].concat()),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Itr2.esm".into()).unwrap()];
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
    let mut g = Game::new(&f.data, world, GameConfig::default());
    g.player_entered(Some(FormId(CELL_A)));
    // The player in cell A at the origin.
    query(&mut g, "player.MoveTo TestBeastRef
player.SetPos X 0
player.SetPos Y 0
player.SetPos Z 0");
    assert_eq!(g.cell_of(FormId::PLAYER_REF), Some(FormId(CELL_A)));
    g
}

fn query(g: &mut Game, body: &str) -> XVal {
    g.run_query(&format!("Begin Function {{ }}\n{body}\nEnd\n"), Default::default()).unwrap_or_else(|e| panic!("{body}: {e}"))
}

fn num(g: &mut Game, expr: &str) -> f64 {
    match query(g, &format!("SetFunctionValue ({expr})")) {
        XVal::Num(x) => x,
        v => panic!("{expr}: {v:?}"),
    }
}

fn form(g: &mut Game, expr: &str) -> u32 {
    match query(g, &format!("ref r\nlet r := {expr}\nSetFunctionValue r")) {
        XVal::Ref(x) => x,
        XVal::Num(x) => x as u32,
        v => panic!("{expr}: {v:?}"),
    }
}

/// An element of an array a query makes, by its path (`[0]["name"]`), after a prelude
/// that declares what the expression needs. (An array a query returns is dropped with
/// the query's variables, so its elements are read inside the script.)
fn get(g: &mut Game, prelude: &str, expr: &str, path: &str) -> XVal {
    query(g, &format!("{prelude}array_var a
let a := {expr}
SetFunctionValue a{path}"))
}

fn size(g: &mut Game, prelude: &str, expr: &str) -> usize {
    match query(g, &format!("{prelude}array_var a
let a := {expr}
SetFunctionValue (Ar_Size a)")) {
        XVal::Num(x) => x as usize,
        v => panic!("{expr}: {v:?}"),
    }
}

/// The values of a list a query makes (numbers and forms).
fn values(g: &mut Game, expr: &str) -> Vec<XVal> {
    let n = size(g, "", expr);
    (0..n).map(|i| get(g, "", expr, &format!("[{i}]"))).collect()
}

fn s(v: &str) -> XVal {
    XVal::Str(v.into())
}

#[test]
fn dialogue_rules_speech_and_gestures() {
    let f = fixture();
    let mut g = game(&f);
    // The menu is closed: no entries, no infos.
    assert_eq!(num(&mut g, "AddDialogueTopicEntry \"Ask\" 5"), 0.0);
    assert_eq!(num(&mut g, "GetDisplayedDialogueInfos"), 0.0);
    assert_eq!(num(&mut g, "SetDialogueTopicEntryAlpha 0 128"), 0.0);
    assert_eq!(num(&mut g, "SetDialogueTopicHidden TestTopic 1"), 1.0);
    // Any form is taken [observed: itr probes].
    assert_eq!(num(&mut g, "SetDialogueTopicHidden TestNpc 1"), 1.0);
    assert_eq!(num(&mut g, "SetDialogueTopicOrder TestInfoB 3"), 1.0);
    assert_eq!(num(&mut g, "SetDialogueTopicOrder TestInfoB 1"), 1.0);
    assert_eq!(num(&mut g, "ClearDialogueTopicOverrides"), 3.0);
    assert_eq!(num(&mut g, "ClearDialogueTopicOverrides"), 0.0);
    // ForceSay: the topic's first line, whatever its conditions.
    assert_eq!(num(&mut g, "TestNpcRef.IsSaying TestTopic"), 0.0);
    assert_eq!(num(&mut g, "TestNpcRef.ForceSay TestTopic player"), 1.0);
    assert_eq!(num(&mut g, "TestNpcRef.IsSaying TestTopic"), 1.0);
    assert_eq!(num(&mut g, "TestNpcRef.IsSaying TestInfoA"), 1.0);
    assert_eq!(num(&mut g, "TestNpcRef.IsSaying TestInfoB"), 0.0);
    assert_eq!(num(&mut g, "TestNpcRef.Gesture \"Nod\" 8 2"), 1.0);
    assert_eq!(num(&mut g, "TestNpcRef.Gesture \"wave\" 8 2"), 0.0);
    // An endless or not-a-number gesture is refused.
    assert_eq!(num(&mut g, "TestBeastRef.Gesture \"Nod\" 8 (10 ^ 400)"), 0.0);
    assert_eq!(num(&mut g, "TestBeastRef.Gesture \"Nod\" ((-1) ^ 0.5) 2"), 0.0);
    // An actor has no rigid body at rest [observed: itr probes, the player].
    assert_eq!(num(&mut g, "player.IsRigidBodyAtRest"), 0.0);
    assert_eq!(g.other.itr2.gestures.get(&FormId(NPC_REF)).map(|x| (x.kind.as_str(), x.speed)), Some(("nod", 0.4)));
}

#[test]
fn fake_hits_combat_targets_ammo_aid_and_resurrection() {
    let f = fixture();
    let mut g = game(&f);
    let health = |g: &mut Game| num(g, "TestNpcRef.GetActorValue Health");
    let full = health(&mut g);
    assert_eq!(num(&mut g, "TestNpcRef.FakeHit TestBeastRef 10"), 1.0);
    assert_eq!(health(&mut g), full - 10.0);
    // No damage: the weapon's (7).
    query(&mut g, "TestNpcRef.FakeHit TestBeastRef -1 Gun 0 0 1");
    assert_eq!(health(&mut g), full - 17.0);
    // FakeHitEx: fatigue too.
    let fatigue = num(&mut g, "TestNpcRef.GetActorValue Fatigue");
    query(&mut g, "TestNpcRef.FakeHitEx TestBeastRef 3 20 0");
    assert_eq!(num(&mut g, "TestNpcRef.GetActorValue Fatigue"), fatigue - 20.0);
    assert_eq!(num(&mut g, "Scrap.FakeHit player 3"), 0.0);
    // A damage that is not a finite number: as none given (1 without a weapon).
    let before = health(&mut g);
    query(&mut g, "TestNpcRef.FakeHit TestBeastRef ((-1) ^ 0.5)
TestNpcRef.FakeHit TestBeastRef (10 ^ 400)");
    assert_eq!(health(&mut g), before - 2.0);
    // ForceCombatTarget: combat with the target, named by GetCombatTarget.
    assert_eq!(num(&mut g, "TestNpcRef.ForceCombatTarget TestBeastRef"), 1.0);
    assert_eq!(form(&mut g, "TestNpcRef.GetCombatTarget"), CREA_REF);
    assert_eq!(values(&mut g, "TestNpcRef.GetTargetLastSeenLocation TestBeastRef"), vec![XVal::Num(50.0), XVal::Num(0.0), XVal::Num(0.0)]);
    assert_eq!(num(&mut g, "TestNpcRef.GetTargetInitialLocation player"), 0.0);
    assert_eq!(num(&mut g, "TestNpcRef.ForceCombatTarget"), 1.0);
    // RefillAmmo: the gun's ammo, the magazine topped to the clip.
    assert_eq!(num(&mut g, "player.RefillAmmo 20"), 0.0);
    query(&mut g, "player.AddItem Gun 1\nplayer.EquipItem Gun");
    assert_eq!(num(&mut g, "player.RefillAmmo 20"), 1.0);
    assert_eq!(num(&mut g, "player.GetItemCount Bullets"), 20.0);
    assert_eq!(g.other.set_magazine, Some(6));
    // A loaded magazine is topped up, never cut down.
    g.player_magazine = Some(4);
    assert_eq!(num(&mut g, "player.RefillAmmo 1"), 1.0);
    assert_eq!(g.other.set_magazine, Some(5));
    assert_eq!(num(&mut g, "player.RefillAmmo 5"), 1.0);
    assert_eq!(g.other.set_magazine, Some(6));
    assert_eq!(num(&mut g, "player.ForceReload"), 1.0);
    // The engine queues stay bounded while nothing takes them.
    for _ in 0..100 {
        query(&mut g, "player.ForceReload");
    }
    assert_eq!(g.other.itr2.reloads.len(), 64);
    // UseAidItem.
    assert_eq!(num(&mut g, "player.UseAidItem TestChem"), 0.0);
    query(&mut g, "player.AddItem TestChem 2");
    assert_eq!(num(&mut g, "player.UseAidItem TestChem"), 1.0);
    assert_eq!(num(&mut g, "player.GetItemCount TestChem"), 1.0);
    assert_eq!(num(&mut g, "player.UseAidItem Gun"), 0.0);
    // Resurrection.
    query(&mut g, "TestNpcRef.KillActor\nTestBeastRef.KillActor");
    assert_eq!(num(&mut g, "TestNpcRef.GetDead"), 1.0);
    assert_eq!(num(&mut g, "TestNpcRef.ResurrectActorEx"), 1.0);
    assert_eq!(num(&mut g, "TestNpcRef.GetDead"), 0.0);
    assert_eq!(num(&mut g, "ResurrectAll"), 1.0);
    assert_eq!(num(&mut g, "TestBeastRef.GetDead"), 0.0);
    assert_eq!(num(&mut g, "TestNpcRef.Ragdoll 2 0 0 100"), 1.0);
    assert_eq!(g.other.itr2.ragdolls.last().map(|r| (r.limb, r.push)), Some((2, [0.0, 0.0, 100.0])));
}

/// A navmesh answerer: a path through (0, 50, 0), every point at z 0 on the navmesh, two
/// cover points.
struct Nav;
impl NavAsk for Nav {
    fn path(&self, from: [f32; 3], to: [f32; 3]) -> Option<Vec<[f32; 3]>> {
        (to[0] < 1000.0).then(|| vec![from, [0.0, 50.0, 0.0], to])
    }
    fn on_navmesh(&self, at: [f32; 3], _near: Option<u32>) -> bool {
        at[2] == 0.0
    }
    fn cover(&self, _at: [f32; 3], _radius: f32) -> Vec<([f32; 3], u32)> {
        vec![([300.0, 0.0, 0.0], 0x13), ([0.0, 400.0, 0.0], 0x21)]
    }
}

#[test]
fn paths_cover_ground_doors_and_references_by_distance() {
    let f = fixture();
    let mut g = game(&f);
    // Headless: no path.
    assert_eq!(num(&mut g, "TestNpcRef.CanPathToRef TestBeastRef"), 0.0);
    assert_eq!(num(&mut g, "TestNpcRef.GetPathDistanceToRef TestBeastRef"), -1.0);
    let (len, nodes, can, on, off, cover, nth) = fnv_game::nav_ask::with_nav(&Nav, || {
        (
            num(&mut g, "TestNpcRef.GetPathLength TestBeastRef"),
            num(&mut g, "TestNpcRef.GetPathNodeCount TestBeastRef"),
            num(&mut g, "TestNpcRef.CanPathToRef TestBeastRef"),
            num(&mut g, "IsPointOnNavmesh 1 2 0"),
            num(&mut g, "IsPointOnNavmesh 1 2 5"),
            num(&mut g, "Ar_Size (GetCoverPointsInRadius 0 0 0 1000)"),
            values(&mut g, "TestNpcRef.GetNthPathNode TestBeastRef 1"),
        )
    });
    // (100,0,0) → (0,50,0) → (50,0,0).
    let expect = (100f64.powi(2) + 50f64.powi(2)).sqrt() + (50f64.powi(2) + 50f64.powi(2)).sqrt();
    assert!((len - expect).abs() < 1e-3, "{len}");
    assert_eq!((nodes, can, on, off, cover), (3.0, 1.0, 1.0, 0.0, 2.0));
    assert_eq!(nth, vec![XVal::Num(0.0), XVal::Num(50.0), XVal::Num(0.0)]);
    assert_eq!(num(&mut g, "GetCoverPointsInRadius 0 0 0 9000"), 0.0);
    // Cover from a threat in the south: a wall stands between it and points north of y 100.
    let wall = |a: [f32; 3], b: [f32; 3], _k: fnv_game::ray::CastKind| {
        (b[1] > 100.0).then(|| fnv_game::ray::Hit { distance: (100.0 - a[1]).max(0.0), point: [a[0], 100.0, a[2]], form: None, material: None })
    };
    let best = fnv_game::ray::with_caster(&wall, || fnv_game::nav_ask::with_nav(&Nav, || get(&mut g, "", "GetBestCoverFromThreat 0 -500 0 0 0 0 2000", "[0][1]")));
    assert_eq!(best, XVal::Num(400.0));
    // Ground: flat at z -40 under everything.
    let ground = |a: [f32; 3], _b: [f32; 3], _k: fnv_game::ray::CastKind| Some(fnv_game::ray::Hit { distance: a[2] + 40.0, point: [a[0], a[1], -40.0], form: None, material: None });
    let (d, moved, after) = fnv_game::ray::with_caster(&ground, || (num(&mut g, "TestNpcRef.GetDistanceToGround"), num(&mut g, "TestNpcRef.MoveToGround 10"), num(&mut g, "TestNpcRef.GetPos Z")));
    assert_eq!((d, moved, after), (40.0, 1.0, -10.0));
    // Interior: no terrain distance.
    assert_eq!(fnv_game::ray::with_caster(&ground, || num(&mut g, "TestNpcRef.GetDistanceToTerrain")), 0.0);
    // Doors: out through cell B; the next door towards the exterior's door.
    assert_eq!(form(&mut g, "GetRefExteriorDoor TestNpcRef"), DOOR_EXT);
    assert_eq!(form(&mut g, "GetRefExteriorDoor DoorExt"), DOOR_EXT);
    assert_eq!(form(&mut g, "GetRefNextTeleportDoor DoorExt"), DOOR_A);
    assert_eq!(form(&mut g, "GetRefNextTeleportDoor TestNpcRef"), 0);
    // References by distance from the player (at the origin): the creature then the NPC.
    let near = values(&mut g, "GetRefsSortedByDistance 500 200");
    assert_eq!(near, vec![XVal::Ref(CREA_REF), XVal::Ref(NPC_REF)]);
    assert_eq!(values(&mut g, "GetRefsSortedByDistance 60 200"), vec![XVal::Ref(CREA_REF)]);
    assert_eq!(values(&mut g, "GetRefsSortedByDistance 500 0 -1 0 0 0 TestNpc"), vec![XVal::Ref(NPC_REF)]);
    assert_eq!(values(&mut g, "GetRefsSortedByDistance 500 200 -1 0 0 1"), vec![XVal::Ref(CREA_REF)]);
}

#[test]
fn perk_eligibility_with_and_without_a_delta() {
    let f = fixture();
    let mut g = game(&f);
    // Level 1, Guns below 45: blocked by the level and the skill.
    assert_eq!(get(&mut g, "", "GetPerkEligibility GunPerk", "[\"status\"]"), s("blocked"));
    assert_eq!(get(&mut g, "", "GetPerkEligibility GunPerk", "[\"eligible\"]"), XVal::Num(0.0));
    assert_eq!(size(&mut g, "", "GetPerkBlockers GunPerk"), 2);
    assert_eq!(get(&mut g, "", "GetPerkBlockers GunPerk", "[0][\"kind\"]"), s("Level"));
    assert_eq!(get(&mut g, "", "GetPerkBlockers GunPerk", "[1][\"kind\"]"), s("ActorValue"));
    assert_eq!(get(&mut g, "", "GetPerkBlockers GunPerk", "[0][\"missingValue\"]"), XVal::Num(1.0));
    assert_eq!(get(&mut g, "", "GetPerkBlockers GunPerk", "[1][\"requiredValue\"]"), XVal::Num(45.0));
    // A delta of level 2 and Guns 50: takeable.
    let delta = "array_var d
array_var av
let d := ar_Construct StringMap
let av := ar_Construct Map
let av[41] := 50
let d[\"level\"] := 2
let d[\"av\"] := av
";
    assert_eq!(get(&mut g, delta, "GetPerkEligibility GunPerk d", "[\"status\"]"), s("takeable"));
    assert_eq!(get(&mut g, delta, "GetPerkEligibility GunPerk d", "[\"nextRank\"]"), XVal::Num(1.0));
    // The free perk is not a level-up perk unless flag 16; the trait needs flag 4.
    // A data gate is a blocker: "blocked" [observed: itr probes, WildWasteland].
    assert_eq!(get(&mut g, "", "GetPerkEligibility FreePerk", "[\"status\"]"), s("blocked"));
    let empty = "array_var d
let d := ar_Construct StringMap
";
    assert_eq!(get(&mut g, empty, "GetPerkEligibility FreePerk d 16", "[\"status\"]"), s("takeable"));
    assert_eq!(size(&mut g, "", "GetEligiblePerks"), 0);
    assert_eq!(size(&mut g, delta, "GetEligiblePerks d 20"), 3);
    let names: Vec<XVal> = (0..3).map(|i| get(&mut g, delta, "GetEligiblePerks d 20", &format!("[{i}][\"name\"]"))).collect();
    assert_eq!(names, vec![s("FreePerk"), s("TraitPerk"), s("GunPerk")]);
    // Owned at its rank: owned_max.
    query(&mut g, "player.AddPerk GunPerk");
    assert_eq!(get(&mut g, "", "GetPerkEligibility GunPerk", "[\"status\"]"), s("owned_max"));
    // Conditions: the level gate, the skill requirement, the entry point's list filter.
    assert_eq!(size(&mut g, "", "GetPerkConditions GunPerk"), 3);
    let sources: Vec<XVal> = (0..3).map(|i| get(&mut g, "", "GetPerkConditions GunPerk", &format!("[{i}][\"source\"]"))).collect();
    assert_eq!(sources, vec![s("PerkDataRequirement"), s("TopLevelRequirement"), s("EntryPointFilter")]);
    assert_eq!(get(&mut g, "", "GetPerkConditions GunPerk", "[1][\"function\"]"), s("GetActorValue"));
    assert_eq!(get(&mut g, "", "GetPerkConditions GunPerk", "[1][\"argValue\"]"), XVal::Num(41.0));
    assert_eq!(get(&mut g, "", "GetPerkConditions GunPerk", "[2][\"tabIndex\"]"), XVal::Num(1.0));
    // Perks for the gun: GunPerk's damage entry filters on the gun's list.
    assert_eq!(size(&mut g, "", "GetPerksForForm Gun"), 1);
    assert_eq!(get(&mut g, "", "GetPerksForForm Gun", "[0][\"status\"]"), s("owned_active"));
    assert_eq!(get(&mut g, "", "GetPerksForForm Gun", "[0][\"matchedEntryPoints\"][0][\"match\"]"), s("filter"));
    assert_eq!(size(&mut g, "", "GetPerksForForm Scrap"), 0);
}

#[test]
fn recipes_containers_duplicates_races_and_menus() {
    let f = fixture();
    let mut g = game(&f);
    assert_eq!(num(&mut g, "Ar_Size (GetAvailableRecipes)"), 0.0);
    query(&mut g, "player.AddItem Scrap 2");
    assert_eq!(values(&mut g, "GetAvailableRecipes Bench"), vec![XVal::Ref(RCPE)]);
    assert_eq!(values(&mut g, "GetAvailableRecipes TestNpc"), vec![XVal::Ref(RCPE)]);
    assert_eq!(num(&mut g, "player.GetVisibleContainerInventoryCount"), 1.0);
    let copy = form(&mut g, "TestBeastRef.Duplicate");
    assert!(copy != 0 && copy != CREA_REF);
    assert_eq!(g.world.peek(FormId(copy)).map(|s| s.base), Some(FormId(CREA)));
    assert_eq!(num(&mut g, "TestNpcRef.SetRaceAlt OtherRace"), 1.0);
    assert_eq!(form(&mut g, "TestNpcRef.GetRace"), RACE2);
    assert_eq!(num(&mut g, "TestBeastRef.SetRaceAlt OtherRace"), 0.0);
    assert_eq!(num(&mut g, "TestNpcRef.ShowBarterMenuWhitelist GunList"), 1.0);
    assert_eq!(g.other.itr2.barter_filter, Some((FormId(FLST), true)));
    assert_eq!(g.barter_requests.last().map(|b| b.merchant), Some(FormId(NPC_REF)));
    assert_eq!(num(&mut g, "StartNewGame"), 1.0);
    assert!(g.other.itr2.start_new_game);
    query(&mut g, "TAP");
    assert!(g.other.itr2.primitives_shown);
    assert_eq!(num(&mut g, "CenterOnCellAlt \"CellB\""), 1.0);
}

/// ObjectUnderReticle casts the crosshair's ray between its two ranges, out to 50000
/// units, and gives the first reference it meets [documented: ObjectUnderReticle].
#[test]
fn object_under_reticle_casts_the_crosshairs_ray_between_its_ranges() {
    let f = fixture();
    let mut g = game(&f);
    // No ray told (headless): nothing.
    assert_eq!(num(&mut g, "IsFormValid (ObjectUnderReticle 0 2048)"), 0.0);
    g.senses.reticle = Some(fnv_game::functions::senses::Reticle { start: [0.0, 0.0, 0.0], dir: [0.0, 1.0, 0.0], hit: None });
    // An actor 1000 units along the ray.
    let npc = |a: [f32; 3], b: [f32; 3], _k: fnv_game::ray::CastKind| {
        (a[1] <= 1000.0 && b[1] >= 1000.0).then(|| fnv_game::ray::Hit { distance: 1000.0 - a[1], point: [0.0, 1000.0, 0.0], form: Some(NPC_REF), material: None })
    };
    let (near, far, beyond, short) = fnv_game::ray::with_caster(&npc, || {
        (
            num(&mut g, "(ObjectUnderReticle 0 2048) == TestNpcRef"),
            num(&mut g, "(ObjectUnderReticle 500 90000) == TestNpcRef"),
            num(&mut g, "(ObjectUnderReticle 1500 4000) == TestNpcRef"),
            num(&mut g, "(ObjectUnderReticle 0 900) == TestNpcRef"),
        )
    });
    assert_eq!((near, far, beyond, short), (1.0, 1.0, 0.0, 0.0));
}

/// In an extender expression a string in parentheses names an actor value or an axis,
/// in any case; an unknown name leaves a `let`'s variable as it was [observed: #2276
/// job A, GetAV ("Strength"), GetAV (s), GetAV ($s), SetAV ("Luck") 7, GetPos ("Z")].
#[test]
fn a_string_names_an_actor_value_or_an_axis() {
    let f = fixture();
    let mut g = game(&f);
    let bare = num(&mut g, "player.GetAV Strength");
    assert_eq!(num(&mut g, "player.GetAV (\"strength\")"), bare);
    assert_eq!(query(&mut g, "string_var s\nlet s := \"Strength\"\nSetFunctionValue (player.GetAV (s))"), XVal::Num(bare));
    query(&mut g, "player.SetAV (\"Luck\") 7");
    assert_eq!(num(&mut g, "player.GetAV Luck"), 7.0);
    query(&mut g, "player.SetPos Z 12");
    assert_eq!(num(&mut g, "player.GetPos (\"Z\")"), 12.0);
    assert_eq!(query(&mut g, "float fA\nlet fA := -99\nlet fA := player.GetAV (\"NoSuchAV\")\nSetFunctionValue fA"), XVal::Num(-99.0));
}
