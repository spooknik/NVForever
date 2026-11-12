//! The world snapshot (player-reports.md §4.3): the state most bugs need, as JSON (at most
//! [`MAX_BYTES`]), rebuilt every [`EVERY_SECS`] while reports are on and kept ready, so a
//! panic can write it beside its crash report without touching the game's state
//! (crash.rs; `snapshot-<time>-<pid>.json`), and it is sent with that crash.
//!
//! What it holds:
//! - the player: health and action points, limb conditions, S.P.E.C.I.A.L., level, XP,
//!   karma, equipped items, active effects, faction reputations, position and cell;
//! - player-facing quests running, with their stage and the objectives shown (others counted);
//! - global variables changed from their defaults (at most [`MAX_GLOBALS`]);
//! - the [`MAX_ACTORS`] nearest actors: reference and base, position, dead or unconscious,
//!   current package, combat target, health;
//! - loaded cells and memory use.
//!
//! The game's own values are read the way the Pip-Boy reads them (inside
//! `nvf_play::script::with_world`), never every frame. Not yet: the AI's current procedure
//! (private to residents.rs), the 20 most expensive scripts (nothing times scripts one by one).

use fnv_game::actor_values as av;
use fnv_game::functions as gf;
use fnv_sim::FormId;
use serde_json::{json, Value};
use std::sync::Mutex;

pub const EVERY_SECS: f64 = 5.0;
pub const MAX_BYTES: usize = 256 * 1024;
pub const MAX_ACTORS: usize = 30;
pub const MAX_GLOBALS: usize = 200;

static LATEST: Mutex<String> = Mutex::new(String::new());

/// The latest snapshot's text (empty before the first).
pub fn latest() -> String {
    LATEST.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Write the latest snapshot to `path` (a panic, crash.rs); an error when there is none yet.
pub fn dump(path: &std::path::Path) -> std::io::Result<()> {
    let text = latest();
    if text.is_empty() {
        return Err(std::io::Error::other("no world snapshot yet"));
    }
    std::fs::write(path, super::redact(&text))
}

fn hex(id: FormId) -> String {
    format!("{:08X}", id.0)
}

/// A form's editor ID, else its name, else nothing.
fn label(g: &fnv_game::game::Game, id: FormId) -> Option<String> {
    g.data.edid_of.get(&id).cloned().or_else(|| Some(g.data.name(id)).filter(|n| !n.is_empty()))
}

/// Build the snapshot from the running game (`g` inside `with_world`) and the engine's own state.
pub fn build(g: &mut fnv_game::game::Game, defaults: &[(FormId, f32)], cells: Value, memory: Value) -> Value {
    let player = FormId::PLAYER_REF;
    let value = |g: &mut fnv_game::game::Game, code: u16| gf::actor_value(g, player, code).ok().map(|v| (v * 10.0).round() / 10.0);
    let special: Vec<Option<f64>> = av::SPECIALS.map(|c| value(g, c)).collect();
    let limbs: Vec<Option<f64>> = av::LIMB_CONDITIONS.map(|c| value(g, c)).collect();
    let me = g.world.peek(player).map(|r| (r.pos, r.cell));
    let equipped: Vec<Value> = g
        .world
        .peek(player)
        .and_then(|r| r.inventory.as_ref())
        .map(|inv| inv.equipped())
        .unwrap_or_default()
        .into_iter()
        .map(|id| json!({ "id": hex(id), "name": label(g, id) }))
        .collect();
    let effects: Vec<Value> = g
        .effects
        .iter()
        .filter(|e| e.target == player && !e.inactive)
        .take(64)
        .map(|e| json!({ "effect": hex(e.effect), "name": label(g, e.effect), "source": hex(e.source), "magnitude": e.magnitude, "duration": e.duration, "elapsed": e.elapsed }))
        .collect();
    let reputations: Vec<Value> = g
        .player
        .reputation
        .iter()
        .map(|(f, [fame, infamy])| json!({ "faction": hex(*f), "name": label(g, *f), "fame": fame, "infamy": infamy }))
        .collect();
    let quests: Vec<Value> = g
        .quests
        .iter()
        .filter(|(_, s)| s.running && !s.completed)
        // Player-facing only (a name and objectives, as the Pip-Boy lists them); the many
        // background quests (radio, generic AI) are only counted.
        .filter(|(q, _)| g.data.quests.get(q).is_some_and(|d| d.name.as_deref().is_some_and(|n| !n.trim().is_empty()) && !d.objectives.is_empty()))
        .map(|(q, s)| {
            let objectives: Vec<Value> = s
                .objectives
                .iter()
                .filter(|(_, o)| o.displayed && !o.completed)
                .map(|(i, _)| json!({ "index": i, "text": g.objective_text(*q, *i).map(|t| g.expand_codes(&t)) }))
                .collect();
            json!({ "quest": hex(*q), "name": label(g, *q), "stage": s.stage(), "objectives": objectives })
        })
        .collect();
    let background = g.quests.values().filter(|s| s.running && !s.completed).count() - quests.len();
    let mut globals = Vec::new();
    for (id, default) in defaults {
        if let Ok(v) = g.world.global(*id) {
            if (v - default).abs() > f32::EPSILON {
                globals.push(json!({ "global": hex(*id), "name": label(g, *id), "value": v, "default": default }));
                if globals.len() >= MAX_GLOBALS {
                    break;
                }
            }
        }
    }
    let here = me.map(|(p, _)| p);
    let mut near: Vec<(f32, FormId)> = g
        .extra
        .keys()
        .copied()
        .filter(|id| *id != player)
        .filter_map(|id| {
            let r = g.world.peek(id)?;
            let p = here?;
            let d = ((r.pos[0] - p[0]).powi(2) + (r.pos[1] - p[1]).powi(2) + (r.pos[2] - p[2]).powi(2)).sqrt();
            Some((d, id))
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut actors = Vec::new();
    for (dist, id) in near {
        if actors.len() >= MAX_ACTORS {
            break;
        }
        let Ok(health) = gf::actor_health(g, id) else { continue };
        let base = gf::base_of(g, id);
        let x = g.extra.get(&id);
        let pos = g.world.peek(id).map(|r| r.pos);
        actors.push(json!({
            "ref": hex(id), "edid": label(g, id), "base": base.map(hex), "base_name": base.and_then(|b| label(g, b)),
            "distance": dist.round(), "pos": pos.map(|p| [p[0].round(), p[1].round(), p[2].round()]),
            "health": (health * 10.0).round() / 10.0,
            "dead": x.is_some_and(|x| x.dead), "unconscious": x.is_some_and(|x| x.unconscious),
            "package": x.and_then(|x| x.current_package).map(hex),
            "combat_target": g.fighting_target_of(id).map(hex),
        }));
    }
    json!({
        "schema": "nvf-snapshot/1",
        "player": {
            "pos": me.map(|(p, _)| [p[0].round(), p[1].round(), p[2].round()]),
            "cell": me.and_then(|(_, c)| c).map(hex),
            "health": value(g, av::HEALTH), "action_points": value(g, av::ACTION_POINTS),
            "limbs": limbs, "special": special,
            "level": g.player_level(), "xp": value(g, av::XP), "karma": value(g, av::KARMA),
            "equipped": equipped, "effects": effects, "reputations": reputations,
        },
        "quests": quests,
        "background_quests_running": background,
        "globals_changed": globals,
        "actors": actors,
        "cells": cells,
        "memory": memory,
    })
}

/// Keep `v` as the latest snapshot, within [`MAX_BYTES`] (actors, then globals, then
/// effects are cut first).
pub fn keep(mut v: Value) {
    let mut text = v.to_string();
    for key in ["actors", "globals_changed"] {
        while text.len() > MAX_BYTES {
            let Some(list) = v.get_mut(key).and_then(Value::as_array_mut).filter(|l| !l.is_empty()) else { break };
            list.truncate(list.len() / 2);
            text = v.to_string();
        }
    }
    if text.len() > MAX_BYTES {
        if let Some(p) = v.get_mut("player").and_then(Value::as_object_mut) {
            p.remove("effects");
        }
        text = v.to_string();
    }
    *LATEST.lock().unwrap_or_else(|e| e.into_inner()) = text;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_snapshot_is_cut_to_the_cap() {
        let actors: Vec<Value> = (0..5000).map(|i| json!({ "ref": format!("{i:08X}"), "pad": "x".repeat(200) })).collect();
        keep(json!({ "schema": "nvf-snapshot/1", "player": {}, "actors": actors, "globals_changed": [] }));
        let t = latest();
        assert!(t.len() <= MAX_BYTES, "{}", t.len());
        let v: Value = serde_json::from_str(&t).unwrap();
        assert!(!v["actors"].as_array().unwrap().is_empty(), "the nearest actors stay");
    }
}
