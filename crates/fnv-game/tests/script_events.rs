//! The script blocks OnOpen, OnClose, OnActorEquip and OnMagicEffectHit on a synthetic
//! plugin (no game data); OnSell is never run, as in the original. docs/spec-notes/
//! script-functions.md §3.3.

use crate::common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::combat::HitInfo;
use fnv_game::functions::Ctx;
use fnv_game::game::{event, Game, GameConfig, InstKey};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const CELL: u32 = 0x200;
const BURN_FX: u32 = 0x850;
const OTHER_FX: u32 = 0x851;
const HURT_FX: u32 = 0x852;
const BURNING: u32 = 0x860;
const HURTING: u32 = 0x861;
const DOOR_SCRIPT: u32 = 0x901;
const TRAP_SCRIPT: u32 = 0x902;
const NPC_SCRIPT: u32 = 0x903;
const HURT_SCRIPT: u32 = 0x904;
const HURT_FX_SCRIPT: u32 = 0x905;
const KILL_ON_HIT_SCRIPT: u32 = 0x906;
const FAR_SCRIPT: u32 = 0x907;
const NEAR_SCRIPT: u32 = 0x908;
const DOOR: u32 = 0x700;
const TRAP: u32 = 0x710;
const NPC: u32 = 0x720;
const HURT_NPC: u32 = 0x721;
const KILL_ON_HIT_NPC: u32 = 0x722;
const FAR_NPC: u32 = 0x723;
const NEAR_NPC: u32 = 0x724;
const ITEM: u32 = 0x730;
const BOX: u32 = 0x740;
const DOOR_REF: u32 = 0x300;
const BOX_REF: u32 = 0x302;
const TRAP_REF: u32 = 0x301;
const NPC_REF: u32 = 0x320;
const DAMAGED_REF: u32 = 0x321;
const HIT_REF: u32 = 0x322;
const KILL_ON_HIT_REF: u32 = 0x323;
const FAR_REF: u32 = 0x324;
const NEAR_REF: u32 = 0x325;
const GET_IN_SAME_CELL: u16 = 0x1020;
const KILL_ACTOR: u16 = 0x108B;
const DAMAGE_ACTOR_VALUE: u16 = 0x1181;
const GET_HIT_LOCATION: u16 = 0x1187;
const GET_CAUSE_OF_DEATH: u16 = 0x118D;
const GET_KILLING_BLOW_LIMB: u16 = 0x11F0;
const HEALTH: u16 = 16;

fn r(i: u16) -> Vec<u8> {
    [vec![b'r'], le16(i)].concat()
}

/// `If <e>`, its arm `skip` statements long.
fn if_(skip: u16, e: Vec<u8>) -> Vec<u8> {
    st(0x16, &[le16(skip), le16(e.len() as u16), e].concat())
}

fn endif() -> Vec<u8> {
    st(0x19, &[])
}

/// `set <var i> to <var i> + 1`.
fn bump(i: u16) -> Vec<u8> {
    set(var(b's', i), ex(&[var(b's', i), a("1"), a("+")]))
}

/// A function call statement with no parameters.
fn call0(op: u16) -> Vec<u8> {
    st(op, &le16(0))
}

/// `DamageAV Health 9999`.
fn damage_health() -> Vec<u8> {
    st(DAMAGE_ACTOR_VALUE, &[le16(2), le16(HEALTH), vec![b'n'], 9999i32.to_le_bytes().to_vec()].concat())
}

/// A block with one form parameter (the unit's reference `i`).
fn on(i: u16) -> Option<Vec<u8>> {
    Some([le16(1), r(i)].concat())
}

fn mgef(fid: u32, edid: &str) -> Vec<u8> {
    // MGEF DATA (72 bytes): Value Modifier on Health, hostile and detrimental.
    let data = [u32s(&[1 | 4]), vec![0; 4], u32s(&[0]), (-1i32).to_le_bytes().to_vec(), (-1i32).to_le_bytes().to_vec(), vec![0; 4], vec![0; 4], 1f32.to_le_bytes().to_vec(), vec![0; 24], vec![0; 8], u32s(&[0]), 16i32.to_le_bytes().to_vec()].concat();
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", data)])
}

/// A Script effect (archetype 1) running `script`; MGEF DATA as in [`mgef`].
fn script_mgef(fid: u32, edid: &str, script: u32) -> Vec<u8> {
    let data = [u32s(&[0]), vec![0; 4], u32s(&[script]), (-1i32).to_le_bytes().to_vec(), (-1i32).to_le_bytes().to_vec(), vec![0; 4], vec![0; 4], 1f32.to_le_bytes().to_vec(), vec![0; 24], vec![0; 8], u32s(&[1]), (-1i32).to_le_bytes().to_vec()].concat();
    record(b"MGEF", 0, fid, &[sub(b"EDID", z(edid)), sub(b"DATA", data)])
}

fn placed(sig: &[u8; 4], fid: u32, base: u32) -> Vec<u8> {
    record(sig, 0, fid, &[sub(b"NAME", base.to_le_bytes().to_vec()), sub(b"DATA", vec![0; 24])])
}

fn plugin_bytes() -> Vec<u8> {
    // Door: nOpened (1), nClosed (2). Trap: nBurnt (1), nOther (2). NPC: nPowder (1),
    // nBurnt (2), nSold (3), nKilled (4), nSeen (5): OnActivate kills it and then sets
    // nKilled, OnDeath copies nKilled into nSeen.
    let scripts = [
        record(b"SCPT", 0, DOOR_SCRIPT, &[vec![sub(b"EDID", z("DoorScript"))], unit(0, &script(&[(event::ON_OPEN, None, bump(1)), (event::ON_CLOSE, None, bump(2))]), &[(1, "nOpened", 1), (2, "nClosed", 1)], &[])].concat()),
        record(
            b"SCPT",
            0,
            TRAP_SCRIPT,
            &[vec![sub(b"EDID", z("TrapScript"))], unit(0, &script(&[(event::ON_MAGIC_EFFECT_HIT, on(1), bump(1)), (event::ON_MAGIC_EFFECT_HIT, on(2), bump(2))]), &[(1, "nBurnt", 1), (2, "nOther", 1)], &[BURN_FX, OTHER_FX])].concat(),
        ),
        record(
            b"SCPT",
            0,
            NPC_SCRIPT,
            &[
                vec![sub(b"EDID", z("NpcScript"))],
                unit(
                    0,
                    &script(&[
                        (event::ON_ACTOR_EQUIP, on(1), bump(1)),
                        (event::ON_MAGIC_EFFECT_HIT, on(2), bump(2)),
                        (event::ON_SELL, None, bump(3)),
                        (event::ON_ACTIVATE, None, [call0(KILL_ACTOR), set(var(b's', 4), ex(&[a("1")]))].concat()),
                        (event::ON_DEATH, None, set(var(b's', 5), ex(&[var(b's', 4)]))),
                    ]),
                    &[(1, "nPowder", 1), (2, "nBurnt", 1), (3, "nSold", 1), (4, "nKilled", 1), (5, "nSeen", 1)],
                    &[ITEM, BURN_FX],
                ),
            ]
            .concat(),
        ),
        record(b"SCPT", 0, HURT_FX_SCRIPT, &[vec![sub(b"EDID", z("HurtFxScript"))], unit(0x100, &script(&[(event::SCRIPT_EFFECT_START, None, damage_health())]), &[], &[])].concat()),
        // nDamaged (1), nSeen (2): as the NPC's, by damage; OnHit does the same damage.
        record(
            b"SCPT",
            0,
            HURT_SCRIPT,
            &[
                vec![sub(b"EDID", z("HurtScript"))],
                unit(
                    0,
                    &script(&[
                        (event::ON_ACTIVATE, None, [damage_health(), set(var(b's', 1), ex(&[a("1")]))].concat()),
                        (event::ON_DEATH, None, set(var(b's', 2), ex(&[var(b's', 1)]))),
                        (event::ON_HIT, None, damage_health()),
                    ]),
                    &[(1, "nDamaged", 1), (2, "nSeen", 1)],
                    &[],
                ),
            ]
            .concat(),
        ),
        record(b"SCPT", 0, KILL_ON_HIT_SCRIPT, &[vec![sub(b"EDID", z("KillOnHitScript"))], unit(0, &script(&[(event::ON_HIT, None, call0(KILL_ACTOR))]), &[], &[])].concat()),
        // nRuns (1): its GameMode block ran its "not in the player's cell" branch
        // (`if GetInSameCell player == 0`, SCRO 1 the player).
        record(
            b"SCPT",
            0,
            FAR_SCRIPT,
            &[
                vec![sub(b"EDID", z("FarScript"))],
                unit(0, &script(&[(event::GAME_MODE, None, [if_(1, ex(&[xcall(GET_IN_SAME_CELL, &[le16(1), r(1)].concat()), a("0"), a("==")])), bump(1), endif()].concat())]), &[(1, "nRuns", 1)], &[0x14]),
            ]
            .concat(),
        ),
        // nRuns (1): a GameMode block with no such test.
        record(b"SCPT", 0, NEAR_SCRIPT, &[vec![sub(b"EDID", z("NearScript"))], unit(0, &script(&[(event::GAME_MODE, None, bump(1))]), &[(1, "nRuns", 1)], &[])].concat()),
    ]
    .concat();
    let door = record(b"DOOR", 0, DOOR, &[sub(b"EDID", z("TestDoor")), sub(b"SCRI", DOOR_SCRIPT.to_le_bytes().to_vec())]);
    // A container running the door's script (nOpened, nClosed).
    let cont = record(b"CONT", 0, BOX, &[sub(b"EDID", z("TestBox")), sub(b"SCRI", DOOR_SCRIPT.to_le_bytes().to_vec()), sub(b"DATA", [vec![0], 0f32.to_le_bytes().to_vec()].concat())]);
    let trap = record(b"ACTI", 0, TRAP, &[sub(b"EDID", z("TestTrap")), sub(b"SCRI", TRAP_SCRIPT.to_le_bytes().to_vec())]);
    let item = record(b"MISC", 0, ITEM, &[sub(b"EDID", z("TestPowder"))]);
    let npc = record(
        b"NPC_",
        0,
        NPC,
        &[
            sub(b"EDID", z("TestNpc")),
            sub(b"ACBS", [&0u32.to_le_bytes()[..], &[0; 4], &4u16.to_le_bytes(), &[0; 6], &0f32.to_le_bytes(), &[0; 4]].concat()),
            sub(b"DATA", [&100i32.to_le_bytes()[..], &[5, 5, 5, 5, 5, 5, 5]].concat()),
            sub(b"SCRI", NPC_SCRIPT.to_le_bytes().to_vec()),
        ],
    );
    let hurt_npc = record(
        b"NPC_",
        0,
        HURT_NPC,
        &[
            sub(b"EDID", z("TestHurtNpc")),
            sub(b"ACBS", [&0u32.to_le_bytes()[..], &[0; 4], &4u16.to_le_bytes(), &[0; 6], &0f32.to_le_bytes(), &[0; 4]].concat()),
            sub(b"DATA", [&100i32.to_le_bytes()[..], &[5, 5, 5, 5, 5, 5, 5]].concat()),
            sub(b"SCRI", HURT_SCRIPT.to_le_bytes().to_vec()),
        ],
    );
    let kill_on_hit_npc = record(
        b"NPC_",
        0,
        KILL_ON_HIT_NPC,
        &[
            sub(b"EDID", z("TestKillOnHitNpc")),
            sub(b"ACBS", [&0u32.to_le_bytes()[..], &[0; 4], &4u16.to_le_bytes(), &[0; 6], &0f32.to_le_bytes(), &[0; 4]].concat()),
            sub(b"DATA", [&100i32.to_le_bytes()[..], &[5, 5, 5, 5, 5, 5, 5]].concat()),
            sub(b"SCRI", KILL_ON_HIT_SCRIPT.to_le_bytes().to_vec()),
        ],
    );
    let far_npc = record(
        b"NPC_",
        0,
        FAR_NPC,
        &[
            sub(b"EDID", z("TestFarNpc")),
            sub(b"ACBS", [&0u32.to_le_bytes()[..], &[0; 4], &4u16.to_le_bytes(), &[0; 6], &0f32.to_le_bytes(), &[0; 4]].concat()),
            sub(b"DATA", [&100i32.to_le_bytes()[..], &[5, 5, 5, 5, 5, 5, 5]].concat()),
            sub(b"SCRI", FAR_SCRIPT.to_le_bytes().to_vec()),
        ],
    );
    let near_npc = record(
        b"NPC_",
        0,
        NEAR_NPC,
        &[
            sub(b"EDID", z("TestNearNpc")),
            sub(b"ACBS", [&0u32.to_le_bytes()[..], &[0; 4], &4u16.to_le_bytes(), &[0; 6], &0f32.to_le_bytes(), &[0; 4]].concat()),
            sub(b"DATA", [&100i32.to_le_bytes()[..], &[5, 5, 5, 5, 5, 5, 5]].concat()),
            sub(b"SCRI", NEAR_SCRIPT.to_le_bytes().to_vec()),
        ],
    );
    let mut ench = vec![sub(b"EDID", z("Burning")), sub(b"ENIT", [u32s(&[2, 0, 0]), vec![0; 4]].concat())];
    ench.extend([sub(b"EFID", BURN_FX.to_le_bytes().to_vec()), sub(b"EFIT", [u32s(&[1, 0, 5, 1]), 16i32.to_le_bytes().to_vec()].concat())]);
    let mut hurting = vec![sub(b"EDID", z("Hurting")), sub(b"ENIT", [u32s(&[2, 0, 0]), vec![0; 4]].concat())];
    hurting.extend([sub(b"EFID", HURT_FX.to_le_bytes().to_vec()), sub(b"EFIT", [u32s(&[1, 0, 0, 1]), (-1i32).to_le_bytes().to_vec()].concat())]);
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("TestCell"))]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &[placed(b"REFR", DOOR_REF, DOOR), placed(b"REFR", TRAP_REF, TRAP), placed(b"REFR", BOX_REF, BOX), placed(b"ACHR", NPC_REF, NPC), placed(b"ACHR", DAMAGED_REF, HURT_NPC), placed(b"ACHR", HIT_REF, HURT_NPC), placed(b"ACHR", KILL_ON_HIT_REF, KILL_ON_HIT_NPC), placed(b"ACHR", FAR_REF, FAR_NPC), placed(b"ACHR", NEAR_REF, NEAR_NPC)].concat()));
    [
        tes4(),
        top(b"MGEF", &[mgef(BURN_FX, "Burn"), mgef(OTHER_FX, "Other"), script_mgef(HURT_FX, "HurtFx", HURT_FX_SCRIPT)].concat()),
        top(b"ENCH", &[record(b"ENCH", 0, BURNING, &ench), record(b"ENCH", 0, HURTING, &hurting)].concat()),
        top(b"SCPT", &scripts),
        top(b"MISC", &item),
        top(b"ACTI", &trap),
        top(b"CONT", &cont),
        top(b"DOOR", &door),
        top(b"NPC_", &[npc, hurt_npc, kill_on_hit_npc, far_npc, near_npc].concat()),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat()
}

struct Fixture {
    data: GameData,
    forms: FormStore,
    refs: RefTable,
}

fn fixture() -> Fixture {
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Events.esm".into()).unwrap()];
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
    let mut g = Game::new(&f.data, world, GameConfig { strict: true, ..GameConfig::default() });
    g.new_game();
    g.set_loaded([FormId(CELL)].into_iter().collect());
    g
}

fn count(g: &Game, r: u32, i: u16) -> f64 {
    g.var(InstKey::Ref { reference: FormId(r) }, i).num()
}

#[test]
fn a_door_runs_on_open_and_on_close_when_its_open_flag_changes() {
    let f = fixture();
    let mut g = game(&f);
    // The first look only notes the state.
    g.tick();
    assert_eq!((count(&g, DOOR_REF, 1), count(&g, DOOR_REF, 2)), (0.0, 0.0));
    // Opened (by the player, an NPC or SetOpenState: the flag is what is watched).
    g.world.get(FormId(DOOR_REF)).unwrap().open = true;
    g.tick();
    assert_eq!(count(&g, DOOR_REF, 1), 1.0);
    g.tick();
    assert_eq!(count(&g, DOOR_REF, 1), 1.0, "once per opening");
    g.world.get(FormId(DOOR_REF)).unwrap().open = false;
    g.tick();
    assert_eq!((count(&g, DOOR_REF, 1), count(&g, DOOR_REF, 2)), (1.0, 1.0));
}

#[test]
fn a_queued_open_runs_on_the_next_frame() {
    let f = fixture();
    let mut g = game(&f);
    // The engine's container menu opening and closing (`queue_ref_event`).
    g.queue_ref_event(FormId(DOOR_REF), event::ON_OPEN, Some(FormId::PLAYER_REF));
    assert_eq!(count(&g, DOOR_REF, 1), 0.0);
    g.tick();
    assert_eq!(count(&g, DOOR_REF, 1), 1.0);
    g.queue_ref_event(FormId(DOOR_REF), event::ON_CLOSE, Some(FormId::PLAYER_REF));
    g.tick();
    assert_eq!(count(&g, DOOR_REF, 2), 1.0);
}

#[test]
fn the_runners_player_activation_of_a_container_runs_its_on_open_and_on_close() {
    let f = fixture();
    let input: fnv_game::runner::Input = serde_json::from_value(serde_json::json!({
        "format": "fnv.script.actions/1",
        "actions": [
            {"op": "player_activate", "target": format!("0x{BOX_REF:08X}"), "label": "open"},
            {"op": "get_ref", "ref": format!("0x{BOX_REF:08X}"), "fields": ["var.nOpened", "var.nClosed"], "label": "after"}
        ]
    }))
    .unwrap();
    let out = fnv_game::runner::run(&f.data, &f.forms, &f.refs, &input).unwrap();
    let obs = out["observations"].as_array().unwrap();
    let by = |l: &str| obs.iter().find(|o| o["label"] == l).unwrap().clone();
    assert_eq!(by("open")["opened"], format!("0x{BOX_REF:08X}"));
    assert_eq!(by("after")["var.nOpened"], 1.0);
    assert_eq!(by("after")["var.nClosed"], 1.0);
}

#[test]
fn a_magic_effect_hit_runs_the_block_of_that_effect() {
    let f = fixture();
    let mut g = game(&f);
    // A weapon's object effect reaching an object (the Vault 22 traps and the flamer).
    assert!(g.magic_effect_hits(FormId(TRAP_REF), &[FormId(BURNING)], Some(FormId::PLAYER_REF)));
    assert_eq!((count(&g, TRAP_REF, 1), count(&g, TRAP_REF, 2)), (1.0, 0.0));
    assert!(!g.magic_effect_hits(FormId(DOOR_REF), &[FormId(BURNING)], None), "no such block");
    // On an actor, through the effect applied by a hit.
    let applied = g.apply_hit_effect(FormId(NPC_REF), FormId(BURNING), Some(FormId::PLAYER_REF));
    assert_eq!(applied.len(), 1);
    assert_eq!(count(&g, NPC_REF, 2), 1.0);
}

#[test]
fn on_actor_equip_runs_on_the_actor_and_on_sell_never_runs() {
    let f = fixture();
    let mut g = game(&f);
    g.equip_events(FormId(NPC_REF), FormId(ITEM), true);
    assert_eq!(count(&g, NPC_REF, 1), 1.0);
    // Another item: the block's parameter does not accept it.
    g.equip_events(FormId(NPC_REF), FormId(TRAP), false);
    assert_eq!(count(&g, NPC_REF, 1), 1.0);
    // Nothing in the engine fires OnSell; the frames do not run it either.
    g.frames(5);
    assert_eq!(count(&g, NPC_REF, 3), 0.0);
}

#[test]
fn on_death_of_a_kill_actor_waits_for_the_killing_script_to_finish() {
    // One for My Baby: the victim's script calls KillActor and then sets Boone's
    // bGotRevenge; its OnDeath clears the accused when bGotRevenge is still 0. OnDeath
    // runs after the killing script, so it sees what the script set after KillActor.
    let f = fixture();
    let mut g = game(&f);
    assert!(g.ref_event(FormId(NPC_REF), event::ON_ACTIVATE, None, None));
    assert!(g.is_dead(FormId(NPC_REF)));
    assert_eq!(count(&g, NPC_REF, 4), 1.0);
    assert_eq!(count(&g, NPC_REF, 5), 1.0, "OnDeath ran before the rest of the killing block");
}

#[test]
fn on_death_of_a_damage_actor_value_waits_for_the_damaging_script_to_finish() {
    let f = fixture();
    let mut g = game(&f);
    assert!(g.ref_event(FormId(DAMAGED_REF), event::ON_ACTIVATE, None, None));
    assert!(g.is_dead(FormId(DAMAGED_REF)));
    assert_eq!(count(&g, DAMAGED_REF, 1), 1.0);
    assert_eq!(count(&g, DAMAGED_REF, 2), 1.0, "OnDeath ran before the rest of the damaging block");
}

#[test]
fn a_kill_finished_by_an_on_hit_script_is_the_attackers() {
    // The boxing gloves' script does `DamageAV Health 10` in its OnHit block: the hit that
    // ran the block is what killed.
    let f = fixture();
    let mut g = game(&f);
    let info = HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: 0, critical: false, limb: None };
    let r = g.hit(FormId(HIT_REF), 1.0, info);
    assert!(r.died, "{r:?}");
    assert!(r.xp > 0.0, "{r:?}");
    assert_eq!(g.extra[&FormId(HIT_REF)].killer, Some(FormId::PLAYER_REF));
}

#[test]
fn a_kill_finished_by_a_hits_script_effect_is_the_attackers() {
    // The engine applies a hit's effects just before its damage (`hit_actor`).
    let f = fixture();
    let mut g = game(&f);
    // NPC_REF has no OnHit block, so only the effect can take its Health.
    let applied = g.apply_hit_effect(FormId(NPC_REF), FormId(HURTING), Some(FormId::PLAYER_REF));
    assert_eq!(applied.len(), 1);
    assert!(fnv_game::functions::actor_value(&mut g, FormId(NPC_REF), HEALTH).unwrap() <= 0.0, "the effect took it");
    assert!(!g.is_dead(FormId(NPC_REF)), "the hit that follows decides");
    let info = HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: 0, critical: false, limb: None };
    let r = g.hit(FormId(NPC_REF), 1.0, info);
    assert!(r.died, "{r:?}");
    assert!(r.xp > 0.0, "{r:?}");
    assert_eq!(g.extra[&FormId(NPC_REF)].killer, Some(FormId::PLAYER_REF));
}

/// #2445: an actor the AI processes out of the loaded cells runs its GameMode block when
/// asked (`Game::run_far_frame`, its low-process update) if the block tests that it is
/// not in the player's cell (Chauncey's); in a loaded cell it is left to the frame's own
/// run, so it never runs twice in a frame, and a block without that test runs in the
/// loaded cells only, as before.
#[test]
fn an_actor_out_of_the_loaded_cells_runs_its_game_mode_when_processed() {
    let f = fixture();
    let mut g = game(&f);
    assert!(!g.run_far_frame(FormId(FAR_REF)), "its cell is loaded");
    assert_eq!(count(&g, FAR_REF, 1), 0.0);
    g.set_loaded(Default::default());
    assert!(g.run_far_frame(FormId(FAR_REF)));
    assert!(g.run_far_frame(FormId(FAR_REF)));
    assert_eq!(count(&g, FAR_REF, 1), 2.0, "its GameMode ran each time");
    assert!(!g.run_far_frame(FormId(NPC_REF)), "a script without a GameMode block");
    assert!(!g.run_far_frame(FormId(NEAR_REF)), "a GameMode block with no away test");
    assert_eq!(count(&g, NEAR_REF, 1), 0.0);
}

/// #2433: a harmless hit (a Use Weapon package's Do No Damage) runs the target's OnHit
/// block, which may do what it likes (G.I. Blues' thugs fall in theirs), and deals no
/// damage of its own; a target without the block is not touched.
#[test]
fn a_harmless_hit_runs_on_hit_and_deals_no_damage() {
    let f = fixture();
    let mut g = game(&f);
    let by = HitInfo { attacker: Some(FormId(NPC_REF)), weapon: None, location: -1, critical: false, limb: None };
    let health = |g: &mut Game, r: u32| fnv_game::functions::actor_value(g, FormId(r), HEALTH).unwrap();
    // No OnHit block: nothing runs and its Health stays.
    let before = health(&mut g, NPC_REF);
    assert!(!g.harmless_hit(FormId(NPC_REF), by), "NPC_REF's script has no OnHit block");
    assert_eq!(health(&mut g, NPC_REF), before);
    assert!(!g.is_dead(FormId(NPC_REF)));
    // The block runs, with the attacker as OnHit's subject: its Kill is the hit's effect.
    assert!(g.harmless_hit(FormId(KILL_ON_HIT_REF), by), "its OnHit block runs");
    assert!(g.is_dead(FormId(KILL_ON_HIT_REF)), "the block's Kill ran");
}

fn ask(g: &mut Game, on: u32, opcode: u16) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode, on: Some(FormId(on)), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    g.call_function(&ctx, &[]).num()
}

#[test]
fn a_hit_finished_by_its_on_hit_script_is_the_killing_hit() {
    let f = fixture();
    let mut g = game(&f);
    let r = g.hit(FormId(HIT_REF), 1.0, HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: 3, critical: false, limb: None });
    assert!(r.died, "{r:?}");
    assert_eq!(ask(&mut g, HIT_REF, GET_KILLING_BLOW_LIMB), 3.0);
    assert_eq!(ask(&mut g, HIT_REF, GET_CAUSE_OF_DEATH), 4.0, "no weapon");
}

#[test]
fn a_hit_finished_by_its_script_effect_is_the_killing_hit() {
    let f = fixture();
    let mut g = game(&f);
    // NPC_REF has no OnHit block, so only the effect can take its Health.
    g.apply_hit_effect(FormId(NPC_REF), FormId(HURTING), Some(FormId::PLAYER_REF));
    assert!(fnv_game::functions::actor_value(&mut g, FormId(NPC_REF), HEALTH).unwrap() <= 0.0, "the effect took it");
    assert!(!g.is_dead(FormId(NPC_REF)), "the hit that follows decides");
    let r = g.hit(FormId(NPC_REF), 1.0, HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: 3, critical: false, limb: None });
    assert!(r.died, "{r:?}");
    assert_eq!(ask(&mut g, NPC_REF, GET_KILLING_BLOW_LIMB), 3.0);
}

#[test]
fn a_kill_actor_in_an_on_hit_block_is_no_killing_hit() {
    // A hit is running but did not kill: running is no proof of what killed.
    let f = fixture();
    let mut g = game(&f);
    let r = g.hit(FormId(KILL_ON_HIT_REF), 1.0, HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: None, location: 3, critical: false, limb: None });
    assert!(!r.died && g.is_dead(FormId(KILL_ON_HIT_REF)), "{r:?}");
    assert_eq!(ask(&mut g, KILL_ON_HIT_REF, GET_KILLING_BLOW_LIMB), -1.0);
    assert_eq!(ask(&mut g, KILL_ON_HIT_REF, GET_CAUSE_OF_DEATH), -1.0);
    assert_eq!(ask(&mut g, KILL_ON_HIT_REF, GET_HIT_LOCATION), 3.0, "the hit is still its last");
}
