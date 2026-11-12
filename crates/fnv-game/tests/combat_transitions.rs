//! The Detection-tab transition lines a fighter says when its fight ends, and the Combat
//! tab's knockout lines, on a synthetic plugin (no game data): a fighter whose target died
//! says CombatToNormal, never CombatToLost; one whose target got away says CombatToLost,
//! then LostToNormal (docs/spec-notes/combat-dialogue.md §5, docs/design/alert.md).

mod common;

use common::*;
use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::awareness::State;
use fnv_game::cond::{Cmp, Condition, Operand, RunOn};
use fnv_game::data::topic_type;
use fnv_game::detection::{Band, Detection};
use fnv_game::functions::Ctx;
use fnv_game::game::{Game, GameConfig};
use fnv_game::GameData;
use fnv_script::vm::{ArgValue, Value};
use fnv_script::FunctionTable;
use fnv_sim::{FormId, FormStore, RefTable, SimConfig, World};
use serde_json::Value as Json;

const RAIDERS: u32 = 0x900;
const PLAYER_FACTION: u32 = 0x901;
const QUEST: u32 = 0x910;
const RAIDER: u32 = 0x830;
const WATCHMAN: u32 = 0x831;
const CELL: u32 = 0x200;
const RAIDER_REF: u32 = 0x320;
const WATCHMAN_REF: u32 = 0x321;

/// The topics, with one line each (line id = topic id + 0x100).
const TOPICS: [(u32, &str, u8); 9] = [
    (0xA00, "CombatToNormal", topic_type::DETECTION),
    (0xA01, "CombatToLost", topic_type::DETECTION),
    (0xA02, "LostToNormal", topic_type::DETECTION),
    (0xA03, "LostToCombat", topic_type::DETECTION),
    (0xA04, "AlertToNormal", topic_type::DETECTION),
    (0xA05, "KOed", topic_type::COMBAT),
    (0xA06, "Wake", topic_type::COMBAT),
    (0xA07, "Death", topic_type::COMBAT),
    (0xA08, "DeathResponse", topic_type::COMBAT),
];

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn acbs() -> fnv_esm::raw::Subrecord {
    let mut d = 0u32.to_le_bytes().to_vec();
    for v in [50u16, 0, 1, 0, 0, 100] {
        d.extend(v.to_le_bytes());
    }
    d.extend(0f32.to_le_bytes());
    d.extend(0i16.to_le_bytes());
    d.extend(0u16.to_le_bytes());
    sub(b"ACBS", d)
}

fn snam(faction: u32) -> fnv_esm::raw::Subrecord {
    sub(b"SNAM", [u32b(faction), vec![0, 0, 0, 0]].concat())
}

/// AIDT: Aggressive (attacks enemies on sight).
fn aidt() -> fnv_esm::raw::Subrecord {
    sub(b"AIDT", vec![1, 4, 50, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
}

fn topic(id: u32, edid: &str, ty: u8) -> Vec<u8> {
    let dial = record(b"DIAL", 0, id, &[sub(b"EDID", z(edid)), sub(b"FULL", z(edid)), sub(b"PNAM", 50f32.to_le_bytes().to_vec()), sub(b"DATA", vec![ty, 0])]);
    let trdt = [u32b(0), u32b(50), vec![0; 4], vec![1, 0, 0, 0], u32b(0), vec![0; 4]].concat();
    let info = record(
        b"INFO",
        0,
        id + 0x100,
        &[
            sub(b"DATA", vec![ty, 0, 0, 0]),
            sub(b"QSTI", u32b(QUEST)),
            sub(b"TRDT", trdt),
            sub(b"NAM1", z(edid)),
            sub(b"NAM2", z("")),
            sub(b"NAM3", z("")),
            sub(b"NEXT", vec![]),
        ],
    );
    [dial, group(id.to_le_bytes(), 7, &info)].concat()
}

fn plugin_bytes() -> Vec<u8> {
    let factions = [
        record(b"FACT", 0, RAIDERS, &[sub(b"EDID", z("Raiders")), sub(b"XNAM", [u32b(PLAYER_FACTION), 0i32.to_le_bytes().to_vec(), u32b(1)].concat())]),
        record(b"FACT", 0, PLAYER_FACTION, &[sub(b"EDID", z("PlayerFaction"))]),
    ]
    .concat();
    let npcs = [
        record(b"NPC_", 0, 0x7, &[sub(b"EDID", z("Player")), acbs(), snam(PLAYER_FACTION)]),
        record(b"NPC_", 0, RAIDER, &[sub(b"EDID", z("Raider")), acbs(), snam(RAIDERS), aidt()]),
        record(b"NPC_", 0, WATCHMAN, &[sub(b"EDID", z("Watchman")), acbs(), snam(PLAYER_FACTION), aidt()]),
    ]
    .concat();
    let quest = record(b"QUST", 0, QUEST, &[sub(b"EDID", z("GenericCombat")), sub(b"DATA", [vec![1, 50, 0, 0], 0f32.to_le_bytes().to_vec()].concat())]);
    let dial: Vec<u8> = TOPICS.iter().flat_map(|&(id, edid, ty)| topic(id, edid, ty)).collect();
    let placed = |id: u32, base: u32, x: f32| {
        let data = [x, 0.0, 0.0, 0.0, 0.0, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        record(b"ACHR", 0, id, &[sub(b"NAME", u32b(base)), sub(b"DATA", data)])
    };
    let cell = record(b"CELL", 0, CELL, &[sub(b"EDID", z("FightCell"))]);
    let children = group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &[placed(RAIDER_REF, RAIDER, 0.0), placed(WATCHMAN_REF, WATCHMAN, 500.0)].concat()));
    [
        tes4(),
        top(b"FACT", &factions),
        top(b"NPC_", &npcs),
        top(b"QUST", &quest),
        top(b"DIAL", &dial),
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
    let plugins = vec![Plugin::parse(&plugin_bytes(), "Fight.esm".into()).unwrap()];
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
    g.new_game();
    g.player_entered(Some(FormId(CELL)));
    g
}

fn seen(band: Band) -> Detection {
    Detection { value: if band == Band::Detected { 5.0 } else { -50.0 }, band, ..Detection::default() }
}

/// `actor` detects `target` and settles into Combat; the lines and changes so far are
/// cleared.
fn fight(g: &mut Game, actor: FormId, target: FormId) {
    g.observe_threat(actor, target, &seen(Band::Detected), [100.0, 0.0, 0.0]);
    g.settle_awareness();
    assert_eq!(g.awareness.state(actor), State::Combat, "{actor:?} fights {target:?}");
    g.awareness.barks.clear();
    g.awareness.changes.clear();
    g.speech.clear();
}

/// The Detection topics `actor` said a line of since the last call.
fn said(g: &mut Game, actor: FormId) -> Vec<&'static str> {
    let barks = std::mem::take(&mut g.awareness.barks);
    barks.into_iter().filter(|b| b.1 == actor && b.2.is_some()).map(|b| b.0).collect()
}

fn line_of(f: &Fixture, edid: &str) -> FormId {
    let t = TOPICS.iter().find(|t| t.1 == edid).expect("a test topic");
    let line = FormId(t.0 + 0x100);
    assert!(f.data.infos.contains_key(&line), "the {edid} line is loaded");
    line
}

#[test]
fn a_fighter_whose_target_the_player_died_says_combat_to_normal() {
    // Combat → Normal "whenever I have no targets in my combat list (i.e. they're all
    // dead …)" [documented: GECK wiki Category:Detection]: the owner's report of todo 6
    // item 1 (Violet after the player died).
    let f = fixture();
    let mut g = game(&f);
    let raider = FormId(RAIDER_REF);
    fight(&mut g, raider, FormId::PLAYER_REF);
    assert!(g.kill(FormId::PLAYER_REF, Some(raider)));
    // The engine measures nobody this time (the death camera, or tdetect): the dead
    // player is forgotten all the same.
    g.settle_awareness();
    assert_eq!(g.awareness.state(raider), State::Normal);
    assert_eq!(said(&mut g, raider), vec!["CombatToNormal"]);
    let c = g.awareness.take_changes();
    assert!(c.iter().any(|c| c.actor == raider && c.from == State::Combat && c.to == State::Normal), "{c:?}");
    assert!(g.speech.iter().any(|s| s.speaker == raider && s.info == line_of(&f, "CombatToNormal")));
    // Nothing more: no Lost line however long it waits.
    g.speech.clear();
    for _ in 0..40 {
        g.time += 1.0;
        g.settle_awareness();
    }
    assert!(said(&mut g, raider).is_empty());
    assert!(g.awareness.take_changes().iter().all(|c| c.actor != raider), "no change after Normal");
}

#[test]
fn a_dead_player_still_measured_ends_the_fight_the_same_way() {
    // The engine measures the dead player again: no longer a threat (would_attack), the
    // same CombatToNormal.
    let f = fixture();
    let mut g = game(&f);
    let raider = FormId(RAIDER_REF);
    fight(&mut g, raider, FormId::PLAYER_REF);
    assert!(g.kill(FormId::PLAYER_REF, Some(raider)));
    g.observe_threat(raider, FormId::PLAYER_REF, &seen(Band::Detected), [100.0, 0.0, 0.0]);
    g.settle_awareness();
    assert_eq!(said(&mut g, raider), vec!["CombatToNormal"]);
}

#[test]
fn a_fighter_whose_npc_target_died_says_combat_to_normal() {
    let f = fixture();
    let mut g = game(&f);
    let (raider, watchman) = (FormId(RAIDER_REF), FormId(WATCHMAN_REF));
    fight(&mut g, raider, watchman);
    assert!(g.kill(watchman, Some(raider)));
    g.speech.clear();
    g.settle_awareness();
    assert_eq!(g.awareness.state(raider), State::Normal);
    assert_eq!(said(&mut g, raider), vec!["CombatToNormal"]);
}

#[test]
fn a_fighter_whose_living_target_got_away_says_combat_to_lost_then_lost_to_normal() {
    // Combat → Lost: every target undetected and its timer run out, with events in the
    // alert list; Lost → Normal once the alert list is empty [documented:
    // Category:Detection].
    let f = fixture();
    let mut g = game(&f);
    let raider = FormId(RAIDER_REF);
    fight(&mut g, raider, FormId::PLAYER_REF);
    let mut topics = Vec::new();
    for _ in 0..60 {
        g.time += 1.0;
        g.observe_threat(raider, FormId::PLAYER_REF, &seen(Band::Hidden), [3000.0, 0.0, 0.0]);
        g.settle_awareness();
        topics.extend(said(&mut g, raider));
        g.speech.clear();
    }
    assert_eq!(topics, vec!["CombatToLost", "LostToNormal"]);
    assert_eq!(g.awareness.state(raider), State::Normal);
}

#[test]
fn a_lost_fighter_that_sees_its_target_again_says_lost_to_combat() {
    let f = fixture();
    let mut g = game(&f);
    let raider = FormId(RAIDER_REF);
    fight(&mut g, raider, FormId::PLAYER_REF);
    for _ in 0..11 {
        g.time += 1.0;
        g.observe_threat(raider, FormId::PLAYER_REF, &seen(Band::Hidden), [3000.0, 0.0, 0.0]);
        g.settle_awareness();
    }
    assert_eq!(g.awareness.state(raider), State::Lost);
    assert_eq!(said(&mut g, raider), vec!["CombatToLost"]);
    g.speech.clear();
    g.time += 1.0;
    g.observe_threat(raider, FormId::PLAYER_REF, &seen(Band::Detected), [200.0, 0.0, 0.0]);
    g.settle_awareness();
    assert_eq!(said(&mut g, raider), vec!["LostToCombat"]);
}

#[test]
fn a_knocked_out_actor_says_koed_and_wake_when_it_gets_up() {
    // KOed: "loses all their fatigue points and becomes unconscious"; Wake: "wakes from
    // unconsciousness" [documented: GECK wiki Combat Tab].
    use fnv_game::unconscious::{Cause, FATIGUE_DOWN};
    let f = fixture();
    let mut g = game(&f);
    let watchman = FormId(WATCHMAN_REF);
    assert!(g.knock_out(watchman, Cause::Fatigue, Some(FormId(RAIDER_REF))));
    assert!(g.combat_lines.said.iter().any(|s| s.0 == "KOed" && s.1 == watchman && s.2 == Some(line_of(&f, "KOed"))));
    g.combat_lines.said.clear();
    g.speech.clear();
    g.time += FATIGUE_DOWN + 1.0;
    assert_eq!(g.tick_knockouts(0.0), vec![watchman]);
    assert_eq!(g.combat_lines.said, vec![("Wake", watchman, Some(line_of(&f, "Wake")))]);
}

#[test]
fn a_death_line_cut_after_one_cry_ends_early_and_brings_its_answer_forward() {
    // #1539: the engine plays one cry of a long generic death file and tells the game the
    // line ends with it (`Game::cut_line_after`).
    use fnv_game::combat_lines::Due;
    let f = fixture();
    let mut g = game(&f);
    let (raider, watchman) = (FormId(RAIDER_REF), FormId(WATCHMAN_REF));
    assert!(g.kill(raider, Some(FormId::PLAYER_REF)));
    let death = line_of(&f, "Death");
    let remaining = |g: &Game| g.speech.iter().find(|s| s.speaker == raider).map(|s| (s.info, s.remaining));
    let (info, before) = remaining(&g).expect("the raider says its Death line");
    assert_eq!(info, death);
    let cut = before / 4.0;
    g.combat_lines.responses.push(Due { at: g.time + before, who: watchman, about: raider, topic: "DeathResponse" });
    g.combat_lines.responses.push(Due { at: g.time + 3.0, who: raider, about: watchman, topic: "DeathResponse" });
    // Another line, or a longer length: nothing changes.
    g.cut_line_after(raider, line_of(&f, "Wake"), cut);
    g.cut_line_after(raider, death, before + 1.0);
    assert_eq!(remaining(&g), Some((death, before)));
    g.cut_line_after(raider, death, cut);
    assert_eq!(remaining(&g), Some((death, cut)));
    assert!((g.combat_lines.responses[0].at - (g.time + cut)).abs() < 1e-9, "the answer comes when the cry ends");
    assert_eq!(g.combat_lines.responses[1].at, g.time + 3.0, "an answer about someone else stays");
}

#[test]
fn death_responses_wait_for_each_other() {
    // #2156 (owner's playtest 2026-11-11): a DeathResponse was said after every death.
    // One is said, then none for DEATH_RESPONSE_SECONDS, whoever would say it.
    use fnv_game::combat_lines::{Due, DEATH_RESPONSE_SECONDS};
    let f = fixture();
    let mut g = game(&f);
    let (raider, watchman) = (FormId(RAIDER_REF), FormId(WATCHMAN_REF));
    let answer = line_of(&f, "DeathResponse");
    let said = |g: &Game| g.combat_lines.said.iter().filter(|s| *s == &("DeathResponse", watchman, Some(answer))).count();
    let due = |g: &mut Game| {
        g.speech.clear();
        let at = g.time;
        g.combat_lines.responses.push(Due { at, who: watchman, about: raider, topic: "DeathResponse" });
        g.tick();
    };
    due(&mut g);
    assert_eq!(said(&g), 1, "the first death is answered");
    due(&mut g);
    assert_eq!(said(&g), 1, "a death soon after is answered by nobody");
    assert!(g.combat_lines.responses.is_empty(), "and its answer does not wait: it is dropped");
    g.time += DEATH_RESPONSE_SECONDS;
    due(&mut g);
    assert_eq!(said(&g), 2, "once the wait is over a death is answered again");
}

#[test]
fn a_fight_survives_a_save_and_load() {
    // #971: whoever fought before a save still fights after the load, at the same target,
    // and IsInCombat on the player reads 1 before any detection round has measured again
    // (headless runs never measure; the engine's first script frame comes before it).
    let f = fixture();
    let mut g = game(&f);
    let (raider, watchman) = (FormId(RAIDER_REF), FormId(WATCHMAN_REF));
    fight(&mut g, raider, FormId::PLAYER_REF);
    g.start_combat(watchman, raider);
    assert!(g.awareness.hunted(FormId::PLAYER_REF));
    let snap = g.snapshot();
    let json = serde_json::to_string(&snap).unwrap();
    let back: fnv_game::game::GameSnapshot = serde_json::from_str(&json).unwrap();
    let world = World::restore(&f.forms, &f.refs, &back.sim).unwrap();
    let mut h = Game::restore(&f.data, world, g.cfg, &back).unwrap();
    assert_eq!(h.awareness.actors, g.awareness.actors, "states, lists and targets fought");
    assert_eq!(h.awareness.forced, g.awareness.forced, "StartCombat's pair");
    assert_eq!(h.awareness.state(raider), State::Combat);
    assert_eq!(h.awareness.state(watchman), State::Combat);
    assert!(h.awareness.hunted(FormId::PLAYER_REF), "the player is still a target");
    assert_eq!(h.combat_target_of(raider), Some(FormId::PLAYER_REF));
    assert_eq!(h.combat_target_of(watchman), Some(raider));
    // A settle with nobody measured goes on as it would have without the load.
    g.settle_awareness();
    h.settle_awareness();
    assert_eq!(h.awareness.state(raider), g.awareness.state(raider));
    // A save with nobody fighting writes no awareness, and an older save without the
    // fields still loads with nobody fighting.
    let calm = game(&f).snapshot();
    let calm_json = serde_json::to_value(&calm).unwrap();
    assert!(calm_json.get("awareness").is_none() && calm_json.get("forced_combat").is_none());
    let mut old = serde_json::to_value(&snap).unwrap();
    old.as_object_mut().unwrap().remove("awareness");
    old.as_object_mut().unwrap().remove("forced_combat");
    let old: fnv_game::game::GameSnapshot = serde_json::from_value(old).unwrap();
    let world = World::restore(&f.forms, &f.refs, &old.sim).unwrap();
    let o = Game::restore(&f.data, world, GameConfig::default(), &old).unwrap();
    assert!(!o.awareness.hunted(FormId::PLAYER_REF));
}

// ---- what scripts read of a fight (#2121) ----------------------------------------------

const IS_IN_COMBAT: u16 = 0x1121;
const SET_ALERT: u16 = 0x105A;
const GET_IS_ALERTED: u16 = 0x105B;
const GET_COMBAT_TARGET: u16 = 0x10E8;
const GET_CURRENT_AI_PACKAGE: u16 = 0x106E;
const GET_CURRENT_AI_PROCEDURE: u16 = 0x108F;
const STOP_COMBAT: u16 = 0x1017;
const GET_IS_REFERENCE: u16 = 0x1088;

/// A script function on `on`, its value as a number (a reference: its FormID).
fn call(g: &mut Game, op: u16, on: FormId, args: &[ArgValue]) -> f64 {
    let ctx = Ctx { unit: "test".into(), opcode: op, on: Some(on), self_ref: None, action_ref: None, quest: None, inst: None, in_condition: false };
    match g.call_function(&ctx, args) {
        Value::Num(x) => x,
        Value::Ref(f) => f as f64,
    }
}

fn num(x: f64) -> [ArgValue; 1] {
    [ArgValue::Value(Value::Num(x))]
}

fn target(g: &mut Game, on: FormId) -> f64 {
    call(g, GET_COMBAT_TARGET, on, &[])
}

fn cond(function: u16, run_on: RunOn, params: Vec<ArgValue>, rhs: f64, or: bool) -> Condition {
    Condition { cmp: Cmp::Eq, or, run_on, function: function - 0x1000, params, rhs: Operand::Value(rhs) }
}

/// Idle gates written for the test in the shape of the creatures' combat idles (fnv-game
/// `idles.rs`, `creature_combat_idles_are_gated_by_get_is_alerted_alone_or_or_joined`):
/// GetIsAlerted == 1 alone, and GetIsAlerted == 1 OR procedure 13 OR procedure 16.
fn idle_gates(g: &mut Game, on: FormId) -> (bool, bool) {
    let alone = [cond(GET_IS_ALERTED, RunOn::Subject, vec![], 1.0, false)];
    let or_fight = [cond(GET_IS_ALERTED, RunOn::Subject, vec![], 1.0, true), cond(GET_CURRENT_AI_PROCEDURE, RunOn::Subject, vec![], 13.0, true), cond(GET_CURRENT_AI_PROCEDURE, RunOn::Subject, vec![], 16.0, false)];
    (g.conditions_pass(&alone, Some(on), None, "test"), g.conditions_pass(&or_fight, Some(on), None, "test"))
}

/// A condition run on the subject's combat target: it is the player.
fn target_is_player(g: &mut Game, on: FormId) -> bool {
    let c = [cond(GET_IS_REFERENCE, RunOn::CombatTarget, vec![ArgValue::Value(Value::Ref(FormId::PLAYER_REF.0))], 1.0, false)];
    g.conditions_pass(&c, Some(on), None, "test")
}

/// One second on, `actor` measuring `who` hidden 3,000 units off.
fn hidden_round(g: &mut Game, actor: FormId, who: FormId) {
    g.time += 1.0;
    g.observe_threat(actor, who, &seen(Band::Hidden), [3000.0, 0.0, 0.0]);
    g.settle_awareness();
}

/// Hidden rounds until `actor` is Lost (at most 20).
fn until_lost(g: &mut Game, actor: FormId, who: FormId) {
    for _ in 0..20 {
        if g.awareness.state(actor) == State::Lost {
            break;
        }
        hidden_round(g, actor, who);
    }
    assert_eq!(g.awareness.state(actor), State::Lost);
}

#[test]
fn scripts_read_a_plain_fight_through_hidden_and_lost_until_it_ends() {
    // #2121: in the original an actor fighting the player kept IsInCombat 1, the player
    // as its combat target and procedure COMBAT while GetDetected player read 0, and
    // GetIsAlerted read 0 throughout [observed: npc-behaviour-compare.md §11].
    let f = fixture();
    let mut g = game(&f);
    let raider = FormId(RAIDER_REF);
    let player = FormId::PLAYER_REF.0 as f64;
    fight(&mut g, raider, FormId::PLAYER_REF);
    let reads = |g: &mut Game| (call(g, IS_IN_COMBAT, raider, &[]), call(g, GET_IS_ALERTED, raider, &[]), target(g, raider), call(g, GET_CURRENT_AI_PROCEDURE, raider, &[]), call(g, GET_CURRENT_AI_PACKAGE, raider, &[]));
    // Detected, in Combat.
    assert_eq!(reads(&mut g), (1.0, 0.0, player, 13.0, 5.0), "fighting");
    assert!(target_is_player(&mut g, raider), "Run On Combat Target: the player");
    assert_eq!(idle_gates(&mut g, raider), (false, true), "GetIsAlerted alone fails, the procedure-joined gate passes");
    assert!(g.draws_weapon(raider) && g.alert_stance(raider) && !g.is_alerted(raider), "it still draws and stands alerted");
    // Hidden, still in Combat: the target is kept.
    hidden_round(&mut g, raider, FormId::PLAYER_REF);
    assert_eq!(g.awareness.state(raider), State::Combat);
    assert_eq!(reads(&mut g), (1.0, 0.0, player, 13.0, 5.0), "hidden, still fighting");
    assert!(target_is_player(&mut g, raider));
    // Lost: our search underneath, a fight to scripts.
    until_lost(&mut g, raider, FormId::PLAYER_REF);
    assert_eq!(reads(&mut g), (1.0, 0.0, player, 13.0, 5.0), "Lost reads COMBAT 13 / 5 and keeps the target");
    assert!(target_is_player(&mut g, raider));
    assert_eq!(idle_gates(&mut g, raider), (false, true));
    assert!(g.draws_weapon(raider) && g.alert_stance(raider));
    // Every second of Lost keeps it, until the fight ends as before (no timer of its own).
    let mut ended = false;
    for _ in 0..60 {
        hidden_round(&mut g, raider, FormId::PLAYER_REF);
        if g.awareness.state(raider) == State::Normal {
            ended = true;
            break;
        }
        assert_eq!(target(&mut g, raider), player, "kept through Lost at {}", g.time);
    }
    assert!(ended, "the fight ends");
    assert_eq!(reads(&mut g), (0.0, 0.0, 0.0, 0.0, 0.0), "back to Normal: nothing (no package in the fixture)");
    assert!(!target_is_player(&mut g, raider));
    assert!(!g.draws_weapon(raider) && !g.alert_stance(raider));
}

#[test]
fn get_is_alerted_follows_set_alert_and_itr_alone() {
    // GetIsAlerted is SetAlert's "purely an animation state" [documented: GECK wiki
    // SetAlert], or ITR's SetAlertNS; a fight neither sets nor clears it (#2121).
    let f = fixture();
    let mut g = game(&f);
    let (raider, watchman) = (FormId(RAIDER_REF), FormId(WATCHMAN_REF));
    // Calm, SetAlert 1: alerted, stands and draws so, and the GetIsAlerted gate passes.
    assert_eq!(call(&mut g, GET_IS_ALERTED, watchman, &[]), 0.0);
    call(&mut g, SET_ALERT, watchman, &num(1.0));
    assert_eq!(call(&mut g, GET_IS_ALERTED, watchman, &[]), 1.0);
    assert!(g.alert_stance(watchman) && g.draws_weapon(watchman));
    assert_eq!(idle_gates(&mut g, watchman), (true, true));
    call(&mut g, SET_ALERT, watchman, &num(0.0));
    assert_eq!(call(&mut g, GET_IS_ALERTED, watchman, &[]), 0.0);
    assert!(!g.alert_stance(watchman));
    // ITR's SetAlertNS (no timer).
    g.other.itr.alerts.insert(watchman, None);
    assert_eq!(call(&mut g, GET_IS_ALERTED, watchman, &[]), 1.0);
    g.other.itr.alerts.remove(&watchman);
    // Fighting with SetAlert 1: 1; SetAlert 0 mid-fight: 0, and the fight goes on.
    call(&mut g, SET_ALERT, raider, &num(1.0));
    fight(&mut g, raider, FormId::PLAYER_REF);
    assert_eq!(call(&mut g, GET_IS_ALERTED, raider, &[]), 1.0);
    call(&mut g, SET_ALERT, raider, &num(0.0));
    assert_eq!(call(&mut g, GET_IS_ALERTED, raider, &[]), 0.0);
    assert_eq!(g.awareness.state(raider), State::Combat);
    assert!(g.draws_weapon(raider));
}

#[test]
fn a_kept_combat_target_gives_way_to_a_new_one_death_and_stop_combat_and_itr_comes_first() {
    let f = fixture();
    let (raider, watchman) = (FormId(RAIDER_REF), FormId(WATCHMAN_REF));
    let player = FormId::PLAYER_REF.0 as f64;
    // Another target detected while the player is hidden: it turns to it.
    let mut g = game(&f);
    fight(&mut g, raider, FormId::PLAYER_REF);
    hidden_round(&mut g, raider, FormId::PLAYER_REF);
    assert_eq!(target(&mut g, raider), player);
    g.time += 1.0;
    g.observe_threat(raider, FormId::PLAYER_REF, &seen(Band::Hidden), [3000.0, 0.0, 0.0]);
    g.observe_threat(raider, watchman, &seen(Band::Detected), [500.0, 0.0, 0.0]);
    g.settle_awareness();
    assert_eq!(target(&mut g, raider), watchman.0 as f64, "a new detected target replaces the kept one");
    // The kept target dies: let go at once, before the next round forgets it.
    hidden_round(&mut g, raider, watchman);
    assert_eq!(target(&mut g, raider), watchman.0 as f64, "kept while hidden");
    assert!(g.kill(watchman, None));
    assert_eq!(target(&mut g, raider), 0.0, "a dead target is not read");
    g.settle_awareness();
    assert_eq!(target(&mut g, raider), 0.0);

    // StopCombat ends it.
    let mut g = game(&f);
    fight(&mut g, raider, FormId::PLAYER_REF);
    hidden_round(&mut g, raider, FormId::PLAYER_REF);
    assert_eq!(target(&mut g, raider), player);
    call(&mut g, STOP_COMBAT, raider, &[]);
    assert_eq!(target(&mut g, raider), 0.0);
    assert_eq!(call(&mut g, GET_CURRENT_AI_PROCEDURE, raider, &[]), 0.0);

    // ITR's ForceCombatTarget comes first, in Combat and in Lost.
    let mut g = game(&f);
    fight(&mut g, raider, FormId::PLAYER_REF);
    g.other.itr2.forced_targets.insert(raider, watchman);
    assert_eq!(target(&mut g, raider), watchman.0 as f64);
    until_lost(&mut g, raider, FormId::PLAYER_REF);
    assert_eq!(target(&mut g, raider), watchman.0 as f64);
    // The combat AI's own target (the reports' snapshot) is as before: none once Lost.
    assert_eq!(g.fighting_target_of(raider), None);
}

#[test]
fn an_alert_actor_that_never_fought_reads_search_and_no_target() {
    // Alert alone is left as it was until a capture shows what it reads (#2121).
    let f = fixture();
    let mut g = game(&f);
    let raider = FormId(RAIDER_REF);
    g.observe_threat(raider, FormId::PLAYER_REF, &Detection { value: -10.0, band: Band::Noticed, ..Detection::default() }, [800.0, 0.0, 0.0]);
    g.settle_awareness();
    assert_eq!(g.awareness.state(raider), State::Alert);
    assert_eq!(call(&mut g, GET_CURRENT_AI_PROCEDURE, raider, &[]), 36.0);
    assert_eq!(call(&mut g, GET_CURRENT_AI_PACKAGE, raider, &[]), 29.0);
    assert_eq!(target(&mut g, raider), 0.0);
    assert_eq!(call(&mut g, IS_IN_COMBAT, raider, &[]), 1.0);
    assert!(g.draws_weapon(raider) && g.alert_stance(raider), "it still draws its weapon");
}
