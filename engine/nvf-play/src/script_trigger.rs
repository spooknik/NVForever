//! Trigger volumes for scripts (layer5.md §3.4: OnTriggerEnter / OnTrigger /
//! OnTriggerLeave come from the engine, workstream 5D). A trigger is a scripted
//! reference in a loaded cell whose script has one of those blocks and which carries a
//! placed primitive (`XPRM`, box or sphere): FalloutNV's VCG01VigorTesterTrigger
//! 0x00104C16 is a 216 × 93 × 100 box (half extents) whose OnTriggerEnter sets VCG01
//! stage 60 [observed: `fnvtool esm decode`, `fnvtool script decompile`].
//!
//! A scripted reference whose *model* holds trigger volumes counts too: phantoms
//! (`bhkSimpleShapePhantom`) on the Trigger layer (12), as in the bear trap, the
//! tripwire and the pressure plate (`meshes\traps\beartrap01.nif` node "Trigger": a
//! box 32 × 32 × 8 units half extents) [observed: `fnvtool nif decode`]. Each phantom
//! is the box of its shape's bounds in the model, scaled, turned and moved by the
//! reference [hypothesis: the box shapes under a transform shape are what Havok tests;
//! docs/spec-notes/traps.md §1]. The model is the one the physics cell loader read
//! ([`crate::phys::PhysState::model_triggers`]); until it has, the reference is asked
//! again on the next frame. The volumes follow the reference: when a script moves, turns
//! or rescales it (MoveTo, SetPos, SetAngle, SetScale), they are built again at its new
//! pose [design, #1091].
//!
//! Physics does not hold these volumes (`fnv_phys` loads primitives of CollisionMarker
//! references only, as solid collision); the test here is geometric, each frame:
//! - **Who**: every actor in the loaded cells, not only the player [documented: any actor
//!   fires a trigger, a block parameter such as `Begin OnTriggerEnter player` limits it to
//!   that reference; GECK tutorials, docs/spec-notes/level-design.md §11.2 T1]. The
//!   actors are the player and the placed NPCs and creatures (ACHR, ACRE) of the loaded
//!   cells that are present and alive, at their physics body's feet (else the
//!   simulation's position, else their placed one). The runtime matches the
//!   block parameter against the subject, which is the actor; the action reference is
//!   the actor too. Non-actors (thrown items, projectiles) do not fire them here
//!   [hypothesis for the default; the ActorZone / Projectile flags are not read, T5].
//! - **How**: the actor's collision capsule against the volume ([`Capsule`],
//!   [`crate::script_events::TriggerShape::touches`]) [documented: a collision check of
//!   the object against the volume, T2; hypothesis: the capsule sizes, the player's
//!   physics capsule and `fnv_phys::CharacterParams::default()` × the reference's scale
//!   now (GetScale's, SetScale's included) for other actors].
//! - **When**: OnTriggerEnter once on the frame an actor first touches the volume, then
//!   OnTrigger every frame it touches it (a frame's OnTrigger runs are the script's
//!   per-frame run for GetSecondsPassed, `Game::trigger_frame`), **the entering frame included, after
//!   OnTriggerEnter** [documented: OnTrigger every frame inside, T3; hypothesis: the order
//!   on the entering frame, open in game, level-design.md §13 question 2]; OnTriggerLeave
//!   once on the first frame it no longer does. A trigger that is disabled is left
//!   without a Leave; an actor that is disabled, dies or leaves the loaded cells leaves
//!   its triggers with one [hypothesis]. A trigger whose cell unloads is left without a
//!   Leave by the actors still in it (they stay inside, the cell only went out of reach),
//!   but **the player** leaves it with one: the cell unloads because the player went
//!   away (fast travel, a door, a script's MoveTo) [design, #2449: the Boomers'
//!   NellisArtilleryScript counts the actors in its field in and out, and a player who
//!   fast-travelled out of the field stayed counted, so the barrage never stopped after
//!   the Boomers took the player in; [`leave_reported`]].

use crate::script::Runtime;
use crate::script_events::{trigger_changes, Capsule, TriggerKind, TriggerShape};
use crate::phys::PhysState;
use crate::PlayWorld;
use fnv_game::game::event;
use fnv_phys::CharacterParams;
use fnv_script::vm::Value;
use fnv_sim::FormId;
use fnv_world::transform::ref_rotation;
use std::collections::{BTreeSet, HashMap};

#[derive(Default)]
pub struct Triggers {
    /// The volumes of each candidate, once read (empty: no usable primitive or model
    /// trigger), with the pose they were built at: a reference a script moves, turns
    /// or rescales gets them built again at its new pose (#1091).
    shapes: HashMap<FormId, (Pose, Vec<TriggerShape>)>,
    /// Candidates whose model had not been read yet.
    waiting: HashMap<FormId, Lookup>,
    /// Scripts with a trigger block, by SCPT.
    has_blocks: HashMap<FormId, bool>,
    /// Actor references (ACHR, ACRE) placed in each cell, once listed.
    actors_by_cell: HashMap<FormId, Vec<FormId>>,
    /// (trigger, actor) pairs touching after the last frame.
    pub inside: BTreeSet<(FormId, FormId)>,
}

const TRIGGER_BLOCKS: [u16; 3] = [event::ON_TRIGGER_ENTER, event::ON_TRIGGER, event::ON_TRIGGER_LEAVE];

fn is_actor(sig: &[u8; 4]) -> bool {
    sig == b"ACHR" || sig == b"ACRE"
}

/// A reference's pose as its trigger volumes take it: position, rotation (the reference's
/// angles) and scale.
type Pose = ([f32; 3], [f32; 3], f32);

/// `r`'s pose now: its state's position and rotation (else its placement's) and its
/// scale as GetScale reads it (SetScale's, else the placement's).
fn pose_of(rt: &Runtime, r: FormId) -> Option<Pose> {
    let placed = rt.game.world.placed.get(r)?;
    let (pos, rot) = rt.game.world.peek(r).map_or((placed.pos, placed.rot), |s| (s.pos, s.rot));
    Some((pos, rot, fnv_game::functions::scale_of(&rt.game, r) as f32))
}

/// Where a candidate's volumes come from, once looked up.
enum Lookup {
    /// Its volumes, at its pose when read.
    Shapes(Vec<TriggerShape>),
    /// Its model's trigger volumes, at this pose (position, rotation, scale), once the
    /// model has been read.
    Model(String, [f32; 3], [f32; 9], f32, Pose),
}

/// The trigger volumes of a placed reference at its current pose: its primitive (XPRM),
/// else the Trigger-layer phantoms of its model.
fn lookup(play: &PlayWorld, rt: &Runtime, r: FormId, pose: Pose) -> Lookup {
    let Some(placed) = rt.game.world.placed.get(r) else { return Lookup::Shapes(Vec::new()) };
    let (pos, rot, scale) = pose;
    let rot = ref_rotation(rot);
    // AddPrimitive's and SetPrimitiveBound's (`fnv_game::nvse_jip14`) before the record's.
    if let Some((sphere, b)) = rt.game.script_primitive(r) {
        let kind = if sphere { TriggerKind::Sphere { radius: b[0].abs() } } else { TriggerKind::Box { half_extents: b.map(f32::abs) } };
        return Lookup::Shapes(vec![TriggerShape { pos, rot, scale, kind }]);
    }
    if let Some(p) = fnv_phys::primitive::primitive(play.lo, play.schema, r.0) {
        let kind = match p.kind {
            fnv_phys::primitive::PrimitiveKind::Box => TriggerKind::Box { half_extents: p.bounds.map(f32::abs) },
            fnv_phys::primitive::PrimitiveKind::Sphere => TriggerKind::Sphere { radius: p.bounds[0].abs() },
            _ => return Lookup::Shapes(Vec::new()),
        };
        return Lookup::Shapes(vec![TriggerShape { pos, rot, scale, kind }]);
    }
    match fnv_world::placed::base_object(play.lo, play.schema, placed.base.0).and_then(|b| b.world_model_path()) {
        Some(path) => Lookup::Model(path, pos, rot, scale, pose),
        None => Lookup::Shapes(Vec::new()),
    }
}

/// Each trigger object of a model as a box at the reference's pose: the bounds of its
/// shape in the model, centred where they are, turned and scaled with the reference.
pub fn model_shapes(objects: &[fnv_collide::CollisionObject], pos: [f32; 3], rot: [f32; 9], scale: f32) -> Vec<TriggerShape> {
    objects
        .iter()
        .filter_map(|o| {
            let b = o.root_bounds();
            if !b.is_valid() {
                return None;
            }
            let c = b.center().map(|v| v * scale);
            let rc = [rot[0] * c[0] + rot[1] * c[1] + rot[2] * c[2], rot[3] * c[0] + rot[4] * c[1] + rot[5] * c[2], rot[6] * c[0] + rot[7] * c[1] + rot[8] * c[2]];
            let half = b.extent().map(|v| v / 2.0);
            Some(TriggerShape { pos: [pos[0] + rc[0], pos[1] + rc[1], pos[2] + rc[2]], rot, scale, kind: TriggerKind::Box { half_extents: half } })
        })
        .collect()
}

/// The actors that can fire triggers now, with their capsules: the player (its physics
/// capsule, `player`) and the present, living NPCs and creatures of the loaded cells.
fn actors(rt: &mut Runtime, phys: &PhysState, player: Capsule) -> Vec<(FormId, Capsule)> {
    let placed = rt.game.world.placed;
    let mut ids: BTreeSet<FormId> = BTreeSet::new();
    for c in &rt.game.loaded_cells {
        let list = rt.triggers.actors_by_cell.entry(*c).or_insert_with(|| {
            placed.by_cell.get(c).map_or_else(Vec::new, |v| v.iter().copied().filter(|r| placed.get(*r).is_some_and(|p| is_actor(&p.sig))).collect())
        });
        ids.extend(list.iter().copied());
    }
    let g = &rt.game;
    // Actors a script moved in from elsewhere.
    ids.extend(g.moved.iter().copied().filter(|r| placed.get(*r).is_some_and(|p| is_actor(&p.sig))));
    let npc = CharacterParams::default();
    let mut out = vec![(FormId::PLAYER_REF, player)];
    for r in ids {
        if !g.present(r) || g.extra.get(&r).is_some_and(|e| e.dead) || !g.cell_of(r).is_some_and(|c| g.loaded_cells.contains(&c)) {
            continue;
        }
        let Some(p) = placed.get(r) else { continue };
        // The live feet of its physics body, as the player's capsule; else its state's
        // position. Sized by its scale now, SetScale's included (#1091).
        let feet = phys.npcs.get(&r.0).map(|b| b.ch.feet).unwrap_or_else(|| g.world.peek(r).map_or(p.pos, |s| s.pos));
        let k = (fnv_game::functions::scale_of(g, r) as f32).max(0.0);
        out.push((r, Capsule { feet, radius: npc.radius * k, height: npc.height * k }));
    }
    out
}

/// Test the actors against the triggers of the loaded cells and run the blocks. `player`
/// is the player's capsule; `None` when the player has no position yet.
pub fn update(rt: &mut Runtime, play: &PlayWorld, phys: &PhysState, player: Option<Capsule>) {
    let Some(player) = player else { return };
    let data = rt.game.data;
    let mut candidates: Vec<FormId> = Vec::new();
    for c in &rt.game.loaded_cells {
        for &r in data.scripted_by_cell.get(c).map_or(&[][..], |v| v.as_slice()) {
            let Some(script) = data.script_of_base(rt.game.world.placed.get(r).map_or(FormId(0), |p| p.base)) else { continue };
            let has = *rt.triggers.has_blocks.entry(script).or_insert_with(|| data.script(script).is_some_and(|u| TRIGGER_BLOCKS.iter().any(|&b| u.has_block(b))));
            if has {
                candidates.push(r);
            }
        }
    }
    let mut shapes = Vec::new();
    for r in candidates {
        if !rt.game.present(r) {
            continue;
        }
        let Some(pose) = pose_of(rt, r) else { continue };
        // Moved, turned or rescaled since its volumes were built: build them again.
        if rt.triggers.shapes.get(&r).is_some_and(|(at, _)| *at != pose) {
            rt.triggers.shapes.remove(&r);
            bevy::log::info!("script: trigger {} moved: its volumes follow", rt.game.data.name(r));
        }
        if rt.triggers.waiting.get(&r).is_some_and(|l| matches!(l, Lookup::Model(.., at) if *at != pose)) {
            rt.triggers.waiting.remove(&r);
        }
        if !rt.triggers.shapes.contains_key(&r) {
            let l = match rt.triggers.waiting.remove(&r) {
                Some(l) => l,
                None => lookup(play, rt, r, pose),
            };
            match l {
                Lookup::Shapes(s) => {
                    rt.triggers.shapes.insert(r, (pose, s));
                }
                Lookup::Model(path, pos, rot, scale, at) => match phys.model_triggers(&path) {
                    Some(objects) => {
                        let s = model_shapes(&objects, pos, rot, scale);
                        if !s.is_empty() {
                            bevy::log::info!("script: trigger {} has {} trigger volume(s) in its model", rt.game.data.name(r), s.len());
                        }
                        rt.triggers.shapes.insert(r, (at, s));
                    }
                    // The model is not read yet: ask again on the next frame.
                    None => {
                        rt.triggers.waiting.insert(r, Lookup::Model(path, pos, rot, scale, at));
                        continue;
                    }
                },
            }
        }
        shapes.extend(rt.triggers.shapes[&r].1.iter().map(|s| (r, *s)));
    }
    let mut now = BTreeSet::new();
    if !shapes.is_empty() {
        for (a, cap) in actors(rt, phys, player) {
            for (t, s) in &shapes {
                if s.touches(&cap) {
                    now.insert((*t, a));
                }
            }
        }
    }
    let ch = trigger_changes(&rt.triggers.inside, &now);
    let g = &mut rt.game;
    let who = |g: &fnv_game::Game, a: FormId| if a == FormId::PLAYER_REF { "player".to_string() } else { g.data.name(a) };
    for &(t, a) in &ch.entered {
        bevy::log::info!("script: {} entered trigger {}", who(g, a), g.data.name(t));
        g.ref_event(t, event::ON_TRIGGER_ENTER, Some(Value::Ref(a.0)), Some(a));
    }
    // Each trigger's OnTrigger is its script's run for this frame, once however many
    // actors are inside (`Game::trigger_frame`: GetSecondsPassed counts from it).
    let running: BTreeSet<FormId> = ch.inside.iter().map(|&(t, _)| t).collect();
    for &t in &running {
        g.trigger_frame(t);
    }
    // The runs for the second and later actors inside one trigger come 0 s after its
    // first (`Game::trigger_again`, issue #2455).
    let mut ran = BTreeSet::new();
    for &(t, a) in &ch.inside {
        if !ran.insert(t) {
            g.trigger_again(t);
        }
        g.ref_event(t, event::ON_TRIGGER, Some(Value::Ref(a.0)), Some(a));
    }
    for &(t, a) in &ch.left {
        let loaded = g.cell_of(t).is_some_and(|c| g.loaded_cells.contains(&c));
        if leave_reported(a, g.present(t), loaded) {
            bevy::log::info!("script: {} left trigger {}", who(g, a), g.data.name(t));
            g.ref_event(t, event::ON_TRIGGER_LEAVE, Some(Value::Ref(a.0)), Some(a));
        }
    }
    rt.triggers.inside = now;
}

/// Whether `actor` leaving trigger `t` runs its OnTriggerLeave (module notes): not when
/// the trigger was disabled or deleted (`present` false); when the trigger's cell is no
/// longer loaded, only for the player, who is the one that went away (#2449).
fn leave_reported(actor: FormId, present: bool, loaded: bool) -> bool {
    present && (loaded || actor == FormId::PLAYER_REF)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #2449: the player fast-travelling out of the Boomers' artillery field (its cell
    /// unloads) leaves the trigger with an OnTriggerLeave, so the script counts the
    /// player out; an NPC left behind in an unloaded cell is still inside; a disabled
    /// trigger reports no leave.
    #[test]
    fn the_player_leaves_a_trigger_whose_cell_unloads() {
        let npc = FormId(0x0010_0000);
        assert!(leave_reported(FormId::PLAYER_REF, true, true));
        assert!(leave_reported(npc, true, true));
        assert!(leave_reported(FormId::PLAYER_REF, true, false), "the player went away: the leave runs");
        assert!(!leave_reported(npc, true, false), "an NPC in an unloaded cell stays inside");
        assert!(!leave_reported(FormId::PLAYER_REF, false, true), "a disabled trigger reports none");
        assert!(!leave_reported(FormId::PLAYER_REF, false, false));
    }
}
