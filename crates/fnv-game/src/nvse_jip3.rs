//! JIP LN, JohnnyGuitar and LU functions the owner's mod lists call that no earlier set
//! built (the third batch, 2026-10-27; docs/handoff/modlist-function-counts.json): ash
//! piles, ragdolls, the compass and nearby hostiles, quests, the player's doors and
//! cells, effect shaders and inventory references moved [documented: their pages in the
//! wiki export; observed where a set is named].

use crate::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use fnv_sim::FormId;
use std::collections::HashMap;

/// This file's session state (not saved).
#[derive(Debug, Default)]
pub struct Jip3State {
    /// The last exterior door the player used (GetPCLastExteriorDoor).
    pub last_exterior_door: Option<FormId>,
    /// The player's cell when each script last asked GetPCMovedCell, by the script's
    /// calling reference and quest.
    pub moved_seen: HashMap<(Option<FormId>, Option<FormId>), Option<FormId>>,
    /// JohnnyGuitar's event handlers set here: (event, function, filter form or list).
    pub jg_handlers: Vec<(JgEvent, XVal, Option<FormId>)>,
    /// FreezeTime's mode: game time stands still (`Game::tick`).
    pub time_frozen: bool,
}

/// The JohnnyGuitar events whose handlers this file keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JgEvent {
    /// Every frame (SetJohnnyOnRenderUpdateEventHandler).
    RenderUpdate,
    /// An actor is killed (SetJohnnyOnDyingEventHandler).
    Dying,
    /// A quest is completed (SetJohnnyOnCompleteQuestEventHandler).
    CompleteQuest,
    /// A challenge is completed (SetJohnnyOnChallengeCompleteEventHandler).
    ChallengeComplete,
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(r) if *r != 0 => Some(FormId(*r)),
        _ => None,
    }
}

fn num(args: &[XVal], k: usize, dflt: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(n)) => *n,
        _ => dflt,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(if b { 1.0 } else { 0.0 })
}

/// The answer of a function with output variables (`fnv_script::vm`'s out_call): the
/// return value, then the outputs.
fn outputs(g: &mut Game, ret: XVal, outs: &[f64]) -> XVal {
    let entries = std::iter::once(ret).chain(outs.iter().map(|v| XVal::Num(*v))).enumerate().map(|(i, v)| (fnv_script::vm::XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(fnv_script::vm::ArrayKind::List, entries))
}

/// The way a pitch `x` and heading `z` (radians) look: x positive down, z clockwise from
/// north (+y), as the references' angles are kept.
fn facing(x: f32, z: f32) -> [f32; 3] {
    [z.sin() * x.cos(), z.cos() * x.cos(), -x.sin()]
}

/// The default reach of a cast with no range given [hypothesis].
const RAY_REACH: f32 = 50_000.0;

impl Game<'_> {
    /// JohnnyGuitar's handlers for `ev` on `subject`: each whose filter is none, the form
    /// itself or a form list holding it runs with the form as its argument (no argument for
    /// RenderUpdate) [documented: the SetJohnnyOn…EventHandler pages; hypothesis: the
    /// arguments].
    pub(crate) fn jg_event(&mut self, ev: JgEvent, subject: Option<FormId>) {
        if self.gaps.jip3.jg_handlers.iter().all(|(e, _, _)| *e != ev) {
            return;
        }
        let hs: Vec<(XVal, Option<FormId>)> = self.gaps.jip3.jg_handlers.iter().filter(|(e, _, _)| *e == ev).map(|(_, f, l)| (f.clone(), *l)).collect();
        for (f, filter) in hs {
            let pass = match (filter, subject) {
                (None, _) => true,
                (Some(l), Some(s)) => l == s || crate::functions::list_members(self, l).contains(&s),
                (Some(_), None) => false,
            };
            if !pass {
                continue;
            }
            let args = subject.map(|s| vec![XVal::Ref(s.0)]).unwrap_or_default();
            if let Err(error) = self.run_function(&f, None, args, None, RunCtx::default()) {
                self.emit(crate::trace::EventKind::ScriptError { unit: "JohnnyGuitar event handler".into(), error });
            }
        }
    }

    /// The player used door `door`: kept when it stands outside (GetPCLastExteriorDoor).
    pub(crate) fn note_player_door(&mut self, door: FormId) {
        let outside = self.cell_of(door).is_some_and(|c| self.world.placed.world_of(c).is_some());
        if outside {
            self.gaps.jip3.last_exterior_door = Some(door);
        }
    }
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    let v = match name {
        // The actor an ash or goo pile came from [documented: GetAshPileSource].
        "getashpilesource" => XVal::Ref(subject.and_then(|p| g.ash_pile_source(p)).map_or(0, |a| a.0)),
        // Knocked down (unconscious, pushed away) or dead and lying as a body
        // [documented: GetIsRagdolled; hypothesis: a body counts].
        "getisragdolled" => yes(subject.is_some_and(|a| g.knockout(a).is_some() || g.is_dead(a))),
        // JohnnyGuitar: among the compass's hostiles (`nvse_hud`) [documented:
        // IsCompassHostile].
        "iscompasshostile" => yes(subject.is_some_and(|a| g.compass_hostiles().iter().any(|(r, _)| *r == a))),
        // JohnnyGuitar: a hostile within fHostileActorExteriorDistance (interior:
        // ...InteriorDistance) of the player [documented: IsHostilesNearby].
        "ishostilesnearby" => {
            let inside = g.cell_of(FormId::PLAYER_REF).is_some_and(|c| g.world.placed.world_of(c).is_none());
            let key = if inside { "fHostileActorInteriorDistance" } else { "fHostileActorExteriorDistance" };
            let reach = g.world.setting_f32(key).unwrap_or(0.0);
            yes(g.compass_hostiles().iter().any(|(_, d)| *d <= reach))
        }
        // `quest`: failed, as a Fail Quest stage leaves it (also read as completed)
        // [documented: FailQuest; the completed flag as `Game::set_stage` sets it].
        "failquest" => {
            if let Some(q) = form(args, 0) {
                let s = g.quest(q);
                let first = !s.failed;
                s.failed = true;
                s.completed = true;
                if first {
                    g.emit(crate::trace::EventKind::QuestFailed { quest: q });
                }
            }
            zero
        }
        "getpclastexteriordoor" => XVal::Ref(g.gaps.jip3.last_exterior_door.map_or(0, |d| d.0)),
        // 1 when the player's cell changed since this script last asked; the first call
        // reads 0 [documented: GetPCMovedCell; observed: jip3a, 0 then 0 in one cell].
        "getpcmovedcell" => {
            let now = g.cell_of(FormId::PLAYER_REF);
            let key = (ctx.self_ref, ctx.quest);
            let before = g.gaps.jip3.moved_seen.insert(key, now);
            yes(before.is_some_and(|b| b != now))
        }
        // `shader`: how many times it plays on the reference, from the scripts' plays
        // [documented: HasEffectShader; hypothesis: a magic effect's shader not counted].
        "haseffectshader" => {
            let (Some(r), Some(s)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            XVal::Num(g.script_looks.shaders.iter().filter(|p| p.on == r && p.shader == s).count() as f64)
        }
        // JohnnyGuitar: interacting with it is a crime or an attack: an actor hostile to
        // the player, or something someone else owns [documented: IsCrimeOrEnemy, "the
        // same checks that are used to determine red color of the crosshair"].
        "iscrimeorenemy" => {
            let Some(r) = subject else { return Some(Ok(zero)) };
            let enemy = g.crime_enemy(r) || crate::rest::hostile_to_player(g, r);
            yes(enemy || g.owned_by_other(r).is_some())
        }
        // `[count] [clearOwner] [target]`: the inventory reference's items leave their
        // container, to `target` when given [documented: RemoveMeIRAlt; count <= 0 or
        // none is the whole stack].
        "removemeiralt" => {
            let Some((c, item)) = subject.and_then(|r| g.temp_item(r)) else { return Some(Ok(zero)) };
            let have = g.world.inventory_mut(c).map(|inv| inv.count(item)).unwrap_or(0);
            let want = num(args, 0, 0.0) as i32;
            let n = if want <= 0 { have } else { want.min(have) };
            let gone = g.world.remove_item(c, item, n).unwrap_or(0);
            if let (Some(to), true) = (form(args, 2), gone > 0) {
                let _ = g.world.add_item(to, item, gone);
            }
            zero
        }
        // `[count] [clearOwner]`: the inventory reference's items dropped beside their
        // container, as Drop does [documented: DropMeAlt].
        "dropmealt" => {
            let Some((c, item)) = subject.and_then(|r| g.temp_item(r)) else { return Some(Ok(zero)) };
            let have = g.world.inventory_mut(c).map(|inv| inv.count(item)).unwrap_or(0);
            let want = num(args, 0, 0.0) as i32;
            let n = if want <= 0 { have } else { want.min(have) };
            if n > 0 {
                if let Err(e) = crate::functions::console::call_words(g, Some(c), "Drop", &[format!("{:08X}", item.0), n.to_string()], "DropMeAlt") {
                    return Some(Err(e));
                }
            }
            zero
        }
        // `container [clearOwner]`: a placed item goes into the container [documented:
        // MoveToContainer].
        "movetocontainer" => {
            let (Some(r), Some(to)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            let Some(item) = g.world.peek(r).map(|s| s.base) else { return Some(Ok(zero)) };
            let count = g.world.peek(r).map_or(1, |s| s.count.max(1));
            if g.world.add_item(to, item, count).is_ok() && g.world.set_enabled(r, false).is_ok() {
                let _ = g.world.set_deleted(r, true);
            }
            zero
        }
        // LU `flag [actorBase]`: a Template Data flag (1 Traits … 512 Script). Given a base
        // as the argument it reads 0, with or without a calling reference; on a reference,
        // its base's ACBS template flags when the base has a template (TPLT) [documented:
        // GetActorTemplateFlag; observed: jip3a, six bases as the argument all 0; jip3b,
        // a placed PowderGangerVault19TEMPLATE reads 942 on the reference (its base is one
        // the game made, not the record, whose flags are 128) and 0 with the base given;
        // hypothesis: references placed from the plugins read their record's flags].
        "getactortemplateflag" => {
            if form(args, 1).is_some() {
                return Some(Ok(zero));
            }
            let Some(b) = subject.map(|r| crate::functions::base_of(g, r).unwrap_or(r)) else { return Some(Ok(zero)) };
            if g.data.actors.get(&b).and_then(|a| a.template).is_none_or(|t| t.0 == 0) {
                return Some(Ok(zero));
            }
            let flags = crate::nvse::record_value(g, b, "ACBS", "template_flags").and_then(|v| v.num().ok()).unwrap_or(0.0) as u64;
            yes(flags & (num(args, 0, 0.0) as u64) != 0)
        }
        // The base's Immobile flag (ACBS bit 23) [documented: GetIsImmobile, "incapable of
        // moving"; the bit, the schema's ACBS flags].
        "getisimmobile" => {
            let Some(b) = form(args, 0).or_else(|| subject.map(|r| crate::functions::base_of(g, r).unwrap_or(r))) else { return Some(Ok(zero)) };
            let flags = crate::nvse::record_value(g, b, "ACBS", "flags").and_then(|v| v.num().ok()).unwrap_or(0.0) as u64;
            yes(flags & (1 << 23) != 0)
        }
        // LU `list`: the calling actor's race is in the form list [documented: IsRaceInList].
        "israceinlist" => {
            let (Some(a), Some(l)) = (subject, form(args, 0)) else { return Some(Ok(zero)) };
            let race = g.actor_race(a);
            yes(race.is_some_and(|r| crate::functions::list_members(g, l).contains(&r)))
        }
        // `menu`: the reference the open menu is about [documented: GetMenuTargetRef]: the
        // dialogue menu's speaker (1009); the container (1008), the merchant (1053), the
        // item being repaired (1035) and the weapon being modded (1061, an inventory
        // reference), as the engine reports them (`Game::set_menu_picks`). The other codes
        // read 0 [hypothesis: not kept yet].
        "getmenutargetref" => {
            let code = num(args, 0, 0.0) as u16;
            match code {
                crate::game::menus::DIALOGUE => XVal::Ref(g.dialogue.as_ref().map(|d| d.actor).filter(|_| g.menu_open(code)).map_or(0, |r| r.0)),
                _ => pick_val(g, g.menu_picks(code).target),
            }
        }
        // The form highlighted in the menu on top [documented: GetSelectedItemRef]: an
        // inventory reference in the inventory (1002), container (1008), repair (1035),
        // barter (1053) and mod (1061) menus; 0 in any other [hypothesis: the DATA and
        // STATS pages' base forms are not reported yet].
        "getselecteditemref" => {
            let p = g.top_menu().and_then(|c| g.menu_picks(c).selected);
            pick_val(g, p)
        }
        // `quest seconds`: the quest's script runs after `seconds` more (0: next frame)
        // [documented: SetDelayElapsed; hypothesis: it returns the seconds given].
        "setdelayelapsed" => {
            let Some(q) = form(args, 0) else { return Some(Ok(zero)) };
            let secs = num(args, 1, 0.0).max(0.0);
            let delay = g.quests.get(&q).and_then(|s| s.delay).or_else(|| g.data.quests.get(&q).map(|d| d.script_delay())).unwrap_or(0.0);
            let s = g.quest(q);
            s.since_run = (delay - secs).max(0.0);
            XVal::Num(secs)
        }
        // The open menu's item list is drawn afresh each frame here [design: nothing to
        // refresh] [documented: RefreshItemsList].
        "refreshitemslist" => zero,
        // `mode`: 0 god mode (1 while on), 1 the actors' AI (1 while it runs, 0 after
        // `tai`) [documented: GetDebugModeState; observed: jip3a, 0 then 1 across `tgm`,
        // 1 then 0 then 1 across `tai`; hypothesis: 2 combat AI, 3 collision, 4 borders
        // read 0].
        "getdebugmodestate" => match num(args, 0, -1.0) as i64 {
            0 => yes(g.toggles.god_mode),
            1 => yes(!g.toggles.actors_ai_off),
            _ => zero,
        },
        // `xVar yVar zVar [local]`: the reference's angles in degrees [documented:
        // GetAngleEx; hypothesis: local and global are the same for an unparented
        // reference].
        "getangleex" => {
            let rot = subject.and_then(|r| g.ref_pos_rot(r)).map_or([0.0; 3], |(_, r)| r);
            let d = |a: f32| f64::from(a.to_degrees());
            outputs(g, zero, &[d(rot[0]), d(rot[1]), d(rot[2])])
        }
        // `x0 y0 z0 x1 y1 z1 → pitch heading`: the angles, in degrees, of the way from the
        // first point to the second, pitch positive down as references' [documented:
        // GetAnglesBetweenPoints; hypothesis: the signs and the 0..360 heading].
        "getanglesbetweenpoints" => {
            let p: Vec<f64> = (0..6).map(|k| num(args, k, 0.0)).collect();
            let (dx, dy, dz) = (p[3] - p[0], p[4] - p[1], p[5] - p[2]);
            let pitch = (-dz).atan2(dx.hypot(dy)).to_degrees();
            let heading = dx.atan2(dy).to_degrees().rem_euclid(360.0);
            outputs(g, zero, &[pitch, heading])
        }
        // `x y z pitch heading → x y z [flags] [range]`: where a cast from the point the way
        // of the angles (degrees) first meets the scene; 0 for no hit [documented:
        // GetPointRayCastPos; hypothesis: the second angle is the heading, the collision
        // flags not told apart].
        "getpointraycastpos" => {
            let o = [num(args, 0, 0.0) as f32, num(args, 1, 0.0) as f32, num(args, 2, 0.0) as f32];
            let dir = facing((num(args, 3, 0.0) as f32).to_radians(), (num(args, 4, 0.0) as f32).to_radians());
            let reach = Some(num(args, 6, 0.0) as f32).filter(|r| *r > 0.0).unwrap_or(RAY_REACH);
            let to = [o[0] + dir[0] * reach, o[1] + dir[1] * reach, o[2] + dir[2] * reach];
            match crate::ray::cast_hit(o, to, crate::ray::CastKind::World) {
                Some(h) => outputs(g, XVal::Num(1.0), &[f64::from(h.point[0]), f64::from(h.point[1]), f64::from(h.point[2])]),
                None => outputs(g, zero, &[0.0, 0.0, 0.0]),
            }
        }
        // `xGlobal yGlobal zGlobal [offset] [flags] [node] [range]` and `[flags] [node]`:
        // a cast from the reference along its facing (an actor from its eyes) [documented:
        // GetRayCastPos, GetRayCastRef; hypothesis: the root node's place and the actor's
        // eye 64 above its feet; the node names not read].
        "getraycastpos" | "getraycastref" => {
            let Some(r) = subject else { return Some(Ok(zero)) };
            let Some((pos, rot)) = g.ref_pos_rot(r) else { return Some(Ok(zero)) };
            let actor = r == FormId::PLAYER_REF || g.data.actors.contains_key(&crate::functions::base_of(g, r).unwrap_or(r));
            let dir = facing(rot[0], rot[2]);
            let pos_ray = name == "getraycastpos";
            let offset = if pos_ray { num(args, 3, 0.0) as f32 } else { 0.0 };
            let reach = if pos_ray { Some(num(args, 6, 0.0) as f32).filter(|r| *r > 0.0).unwrap_or(RAY_REACH) } else { RAY_REACH };
            let base = [pos[0], pos[1], pos[2] + if actor { 64.0 } else { 0.0 }];
            let o = [base[0] + dir[0] * offset, base[1] + dir[1] * offset, base[2] + dir[2] * offset];
            let to = [o[0] + dir[0] * reach, o[1] + dir[1] * reach, o[2] + dir[2] * reach];
            let hit = crate::ray::cast_hit(o, to, crate::ray::CastKind::World);
            if pos_ray {
                if let Some(h) = hit {
                    for (k, v) in h.point.iter().enumerate() {
                        if let Some(gl) = form(args, k) {
                            let _ = g.world.set_global(gl, *v);
                        }
                    }
                }
                yes(hit.is_some())
            } else {
                XVal::Ref(hit.and_then(|h| h.form).unwrap_or(0))
            }
        }
        // LU `list type [modIndex] [noClear]`: the list emptied (unless noClear), then every
        // loaded form of the type added, from one plugin when modIndex is given; type 57
        // (cells) only interiors [documented: GetLoadedType].
        "getloadedtype" => {
            let Some(l) = form(args, 0) else { return Some(Ok(zero)) };
            let code = num(args, 1, -1.0) as i32;
            let index = num(args, 2, -1.0) as i64;
            let keep = num(args, 3, 0.0) != 0.0;
            let mut forms = g.data.forms_of_type(code);
            if index >= 0 {
                forms.retain(|f| i64::from(f.0 >> 24) == index);
            }
            if code == 57 {
                forms.retain(|c| g.world.placed.world_of(*c).is_none());
            }
            let list = g.edited_list(l);
            if !keep {
                list.clear();
            }
            for f in forms {
                if !list.contains(&f) {
                    list.push(f);
                }
            }
            zero
        }
        // The effects with a duration on the actor, each as [source, base effect,
        // magnitude, seconds left, caster, duration] [documented: GetTempEffects; hypothesis:
        // the order is the effects' own].
        "gettempeffects" => {
            let Some(a) = subject else { return Some(Ok(zero)) };
            let rows: Vec<Vec<XVal>> = g
                .effects
                .iter()
                .filter(|e| e.target == a && !e.permanent && e.duration > 0.0)
                .map(|e| {
                    vec![
                        XVal::Ref(e.source.0),
                        XVal::Ref(e.effect.0),
                        XVal::Num(f64::from(e.magnitude)),
                        XVal::Num((e.duration - e.elapsed).max(0.0)),
                        XVal::Ref(e.caster.map_or(0, |c| c.0)),
                        XVal::Num(e.duration),
                    ]
                })
                .collect();
            let mut outer = Vec::with_capacity(rows.len());
            for (i, r) in rows.into_iter().enumerate() {
                let inner = r.into_iter().enumerate().map(|(k, v)| (fnv_script::vm::XKey::Num(k as f64), v)).collect();
                outer.push((fnv_script::vm::XKey::Num(i as f64), XVal::Array(g.ext.new_array(fnv_script::vm::ArrayKind::List, inner))));
            }
            XVal::Array(g.ext.new_array(fnv_script::vm::ArrayKind::List, outer))
        }
        // `[on]`: game time stands still while on; with no argument only reads it
        // [documented: FreezeTime; hypothesis: the engine's animations and physics run on].
        "freezetime" => {
            if let Some(XVal::Num(n)) = args.first() {
                g.gaps.jip3.time_frozen = *n != 0.0;
            }
            yes(g.gaps.jip3.time_frozen)
        }
        // `set|remove function flag [filter]`: JohnnyGuitar's handlers kept here
        // (`Game::jg_event`) [documented: the SetJohnnyOn…EventHandler pages].
        "setjohnnyonrendereventhandler" | "setjohnnyonrenderupdateeventhandler" | "setjohnnyondyingeventhandler" | "setjohnnyoncompletequesteventhandler" | "setjohnnyonchallengecompleteeventhandler" => {
            let ev = match name {
                "setjohnnyondyingeventhandler" => JgEvent::Dying,
                "setjohnnyoncompletequesteventhandler" => JgEvent::CompleteQuest,
                "setjohnnyonchallengecompleteeventhandler" => JgEvent::ChallengeComplete,
                _ => JgEvent::RenderUpdate,
            };
            let Some(f) = args.get(1).cloned() else { return Some(Ok(zero)) };
            let filter = form(args, 3);
            g.gaps.jip3.jg_handlers.retain(|(e, h, l)| !(*e == ev && *h == f && *l == filter));
            if num(args, 0, 1.0) != 0.0 {
                g.gaps.jip3.jg_handlers.push((ev, f, filter));
            }
            zero
        }
        _ => return None,
    };
    Some(Ok(v))
}

/// A menu's pick as a script sees it: an inventory reference for an item in a container,
/// else the form.
fn pick_val(g: &mut Game, p: Option<crate::game::MenuPick>) -> XVal {
    match p {
        Some(crate::game::MenuPick::Item { owner, item }) => XVal::Ref(g.temp_ref(owner, item).0),
        Some(crate::game::MenuPick::Form(f)) => XVal::Ref(f.0),
        None => XVal::Ref(0),
    }
}
