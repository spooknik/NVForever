//! Animation groups scripts play on placed objects: `PlayGroup` (fnv-game `anims`;
//! docs/spec-notes/activators.md §5).
//!
//! - **Which objects.** Every drawn object whose model has a named sequence that animates
//!   it gets a [`GroupRig`] (world.rs): those that play an idle or open like a door, and
//!   those with a sequence only scripts play (Forward, Backward, Left, Right, …), which
//!   are drawn with their own node hierarchy for this [design: any reference can be
//!   named by a script, `Ref.PlayGroup`, so the rig does not wait for the first call;
//!   the cost is a node hierarchy per such reference].
//! - **A request** names a group; the sequence of the same name is played
//!   [hypothesis: the names match, activators.md §5]. Its cycle type decides how: a
//!   clamped sequence plays once and holds its end, a looping one loops until another
//!   group replaces it [hypothesis: as the NIF cycle types]. The start flag
//!   [documented: GECK wiki PlayGroup]: 1 and 2 start at once from the beginning (no
//!   activator sequence marks a loop start, so 2 is as 1 [observed: activators.md §5]);
//!   0 lets what plays finish: a group played once until its end, a loop (the object's
//!   own idle included) until its cycle ends. A loop asked for again while it plays
//!   goes on [design].
//! - **What the rules see**: the engine tells `fnv_game` (`ObjectAnims::set_playing`)
//!   when a group cannot be played (no such sequence, the object not drawn) and when one
//!   played once ends; `IsAnimPlaying` answers from that.
//! - **Not drawn**: the request is kept and shown when the object is drawn: a group
//!   played once at its end, a loop from its start [design]. An object a script enabled
//!   at run time (drawn by `nvf_play::script_draw`, which takes a frame or more after
//!   the Enable) shows a group played once where it would be by now, from its start if
//!   it was asked for in the same moment, and the sound keys passed meanwhile play when
//!   it is drawn [design: the request comes with the Enable, and the player watches the
//!   object appear; VMS15's bomber rises out of the lake, `Forward`].
//! - **Drawn again** (after a load, or streamed back in): an object no request waits for
//!   shows the last group a script asked of it (`fnv_game::anims::ObjectAnims::asked`,
//!   saved with the game): a group played once at its end, a loop from its start
//!   [design: a gate a script opened stays open; issue #1396].
//! - **Collision**: the clip each rig plays and its time are published every frame
//!   ([`publish_poses`], `nvf_play::phys::ObjectPoses`); physics poses the keyframed
//!   bodies the sequence moves from it, so the drawn model and its collision cannot
//!   drift (issue #1396).
//! - **Sounds**: the `Sound: <editor ID>` text keys the playing group passes are played
//!   where the object is (anim_sounds.rs, [`ObjectSound`]), keys at the group's very
//!   start included (a tripwire's sound key is 0.00001 s into its Forward).
//! - **Shown and hidden nodes**: the visibility tracks (`NiVisController`) of the clip
//!   playing show and hide the rig's nodes, as on every playing clip
//!   (`nvf_assets::anim_play::Playing::show`); its alpha tracks fade its shapes
//!   (`nvf_assets::material_anim`) [activators.md §6].
//! - **Actors**: a request for an actor (a reference to an NPC or a creature) is handed
//!   to `actor_groups.rs`.
//!
//! A node track on the rig's root is not applied: the root carries the placement
//! [design]. Log: `playgroup: <ref> plays <group> (once|loop, <s> s, flag n)`, and at
//! exit `playgroup summary: …`.

use crate::actor_groups::ActorGroups;
use crate::limbs::ActorRig;
use bevy::prelude::*;
use fnv_game::anims::{AnimRequest, Start};
use fnv_sim::FormId;
use nvf_assets::anim_play::{Clip, Playing};
use nvf_assets::material_anim::RigMaterials;
use nvf_assets::model::PreparedModel;
use nvf_assets::model::ModelCtx;
use nvf_play::script::ScriptState;
use nvf_play::script_draw::ScriptDrawnRig;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// NIF cycle type "clamp": played once [documented: nif.xml CycleType].
const CLAMP: i64 = 2;

/// On the root of a drawn object whose model has named sequences.
#[derive(Component)]
pub struct GroupRig {
    pub form_id: u32,
    model: Arc<PreparedModel>,
    source: String,
    nodes: HashMap<String, Entity>,
    /// The group playing: its code and the index of its clip in the rig's groups.
    current: Option<(u16, usize)>,
    /// A group waiting for what plays to finish (flag 0).
    next: Option<(u16, usize)>,
    /// Sequence time and cycle count last seen, to find passed keys and cycle ends.
    last: Option<(f32, i64)>,
    /// Drawn by `nvf_play::script_draw` (enabled at run time), not render streaming.
    run_time: bool,
}

impl GroupRig {
    pub fn new(form_id: u32, model: Arc<PreparedModel>, source: &str, nodes: HashMap<String, Entity>) -> GroupRig {
        GroupRig { form_id, model, source: source.to_string(), nodes, current: None, next: None, last: None, run_time: false }
    }

    /// The entity of its node named `name` (a node its sequences may move).
    pub fn node(&self, name: &str) -> Option<Entity> {
        self.nodes.get(name).copied()
    }

    fn clip(&self, i: usize) -> Option<&Clip> {
        self.model.rig.as_ref()?.groups.get(i)
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.model.rig.as_ref()?.groups.iter().position(|c| c.name.eq_ignore_ascii_case(name))
    }
}

/// A sound key a playing group passed: SOUN editor ID, where (game units).
#[derive(Message, Clone, Debug)]
pub struct ObjectSound {
    pub at: [f32; 3],
    pub key: String,
}

#[derive(Resource, Default)]
pub struct ObjectAnims {
    /// Requests for objects not drawn, shown when they are, with the time they were made
    /// (`Time::elapsed_secs`).
    waiting: HashMap<u32, (AnimRequest, f32)>,
    /// The last request logged per reference (a script repeating it is not logged again).
    logged: HashMap<u32, (u16, i32)>,
    stats: Stats,
}

#[derive(Default)]
struct Stats {
    asked: u32,
    started: u32,
    queued: u32,
    already: u32,
    ended: u32,
    not_drawn: u32,
    shown_when_drawn: u32,
    restored: u32,
    no_sequence: u32,
    sounds: u32,
    to_actors: u32,
    resets: u32,
    by_group: BTreeMap<String, u32>,
}

/// How far into a clip [`start`] puts it.
#[derive(Clone, Copy, PartialEq)]
enum StartAt {
    /// Its start.
    Start,
    /// Its end, no sound keys.
    End,
    /// `s` seconds after its start (at most its end); the keys it passed play.
    After(f32),
}

/// Start clip `i` of `rig` on its root (replacing what played there).
fn start(commands: &mut Commands, root: Entity, rig: &mut GroupRig, group: u16, i: usize, from: StartAt, mats: Option<&mut RigMaterials>) {
    let Some(clip) = rig.clip(i) else { return };
    let root_name = rig.model.rig.as_ref().and_then(|r| r.nodes.first()).and_then(|n| n.name.clone());
    let mut p = Playing::new(&rig.source, clip, |n| if Some(n) == root_name.as_deref() { None } else { rig.nodes.get(n).copied() });
    match from {
        StartAt::Start => {}
        StartAt::End => p.time = clip.stop,
        StartAt::After(s) => p.time = (clip.start + s.max(0.0)).min(clip.stop),
    }
    let at_end = from == StartAt::End;
    if let Some(m) = mats {
        m.idle = Arc::new(clip.materials.clone());
        m.dark = false;
    }
    let clip_start = clip.start;
    commands.entity(root).insert(p);
    rig.current = Some((group, i));
    rig.next = None;
    // From its start, keys at the start time included; shown at its end, none.
    rig.last = (!at_end).then_some((clip_start - 1.0, 0));
}

fn describe(clip: &Clip) -> String {
    format!("{} ({}, {:.2} s)", clip.name, if clip.cycle_type == CLAMP { "once" } else { "loop" }, clip.length())
}

/// Take the game's requests and play them; follow each playing group's end, its queued
/// successor and its sound keys.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub fn drive(
    mut commands: Commands,
    mut st: ResMut<ObjectAnims>,
    mut script: Option<ResMut<ScriptState>>,
    mut rigs: Query<(Entity, &mut GroupRig, Option<&Playing>, Option<&mut RigMaterials>, &GlobalTransform)>,
    mut sounds: MessageWriter<ObjectSound>,
    actors: Query<&ActorRig>,
    sim: Option<Res<nvf_play::sim::SimState>>,
    mut for_actors: ResMut<ActorGroups>,
    time: Res<Time>,
    mut seen: Local<std::collections::HashSet<Entity>>,
) {
    let st = &mut *st;
    let now = time.elapsed_secs();
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let game = &mut rt.game;
    game.anims.driven = true;
    let requests = std::mem::take(&mut game.anims.requests);
    let by_ref: HashMap<u32, Entity> = rigs.iter().map(|(e, r, ..)| (r.form_id, e)).collect();
    let drawn_actors: std::collections::HashSet<u32> = actors.iter().map(|a| a.reference).collect();
    let is_actor = |r: FormId| {
        drawn_actors.contains(&r.0)
            || sim.as_deref().and_then(|s| s.refs.refs.get(&r).and_then(|p| s.forms.sig(p.base))).is_some_and(|b| &b == b"NPC_" || &b == b"CREA")
    };
    // Rigs started this frame: their new clip is on them from the next frame.
    let mut started: std::collections::HashSet<Entity> = std::collections::HashSet::new();

    // Objects drawn since they were last looked at here that a script asked for while
    // they were not, or asked of before (drawn again after a load or streaming). Not
    // `is_added`: rigs drawn before the scripts run (the first frames of a load) were
    // added while this returned early above.
    if seen.len() > by_ref.len() {
        seen.retain(|e| rigs.contains(*e));
    }
    for (root, mut rig, _, mats, _) in &mut rigs {
        if !seen.insert(root) {
            continue;
        }
        let Some((r, asked)) = st.waiting.remove(&rig.form_id) else {
            let Some((group, name)) = game.anims.asked(FormId(rig.form_id)).map(|(g, n)| (g, n.to_string())) else { continue };
            let Some(i) = rig.find(&name) else { continue };
            let once = rig.clip(i).is_some_and(|c| c.cycle_type == CLAMP);
            let from = if once { StartAt::End } else { StartAt::Start };
            start(&mut commands, root, &mut rig, group, i, from, mats.map(|m| m.into_inner()));
            started.insert(root);
            st.stats.restored += 1;
            if !once {
                game.anims.set_playing(FormId(rig.form_id), Some(group));
            }
            info!("playgroup: {} drawn again, shows {name} {}", game.data.name(FormId(rig.form_id)), if once { "at its end" } else { "from its start" });
            continue;
        };
        let Some(name) = r.name.as_deref() else { continue };
        let Some(i) = rig.find(name) else { continue };
        let once = rig.clip(i).is_some_and(|c| c.cycle_type == CLAMP);
        let from = match (once, rig.run_time) {
            (false, _) => StartAt::Start,
            (true, false) => StartAt::End,
            (true, true) => StartAt::After(now - asked),
        };
        start(&mut commands, root, &mut rig, r.group, i, from, mats.map(|m| m.into_inner()));
        started.insert(root);
        st.stats.shown_when_drawn += 1;
        if from == StartAt::End {
            game.anims.set_playing(r.on, None);
        } else {
            game.anims.set_playing(r.on, Some(r.group));
        }
        match from {
            StartAt::After(s) => info!("playgroup: {} drawn (enabled at run time), plays {name} from {s:.2} s as asked while it was not", game.data.name(r.on)),
            _ => info!("playgroup: {} drawn, shows {name} as asked while it was not", game.data.name(r.on)),
        }
    }

    // Reset3DState: the group stops where it is and nothing waits to be shown (a request
    // made after the reset, in the same frame, still plays below).
    for on in std::mem::take(&mut game.anims.resets) {
        st.waiting.remove(&on.0);
        st.logged.remove(&on.0);
        st.stats.resets += 1;
        let drawn = by_ref.get(&on.0).and_then(|&e| rigs.get_mut(e).ok());
        if let Some((root, mut rig, ..)) = drawn {
            rig.current = None;
            rig.next = None;
            rig.last = None;
            commands.entity(root).remove::<Playing>();
        }
        info!("playgroup: {} reset (Reset3DState)", game.data.name(on));
    }

    for r in requests {
        if is_actor(r.on) && !by_ref.contains_key(&r.on.0) {
            st.stats.to_actors += 1;
            for_actors.queue.push(r);
            continue;
        }
        st.stats.asked += 1;
        let who = game.data.name(r.on);
        let group = r.name.clone().unwrap_or_else(|| format!("code {}", r.group));
        *st.stats.by_group.entry(group.clone()).or_default() += 1;
        let fresh = st.logged.insert(r.on.0, (r.group, r.start.flag())) != Some((r.group, r.start.flag()));
        let Some(&e) = by_ref.get(&r.on.0) else {
            st.stats.not_drawn += 1;
            if fresh {
                info!("playgroup: {who} plays {group} (flag {}): not drawn, shown when it is", r.start.flag());
            }
            game.anims.set_playing(r.on, None);
            st.waiting.insert(r.on.0, (r, now));
            continue;
        };
        let Ok((root, mut rig, playing, mats, _)) = rigs.get_mut(e) else { continue };
        let Some(i) = r.name.as_deref().and_then(|n| rig.find(n)) else {
            st.stats.no_sequence += 1;
            if fresh {
                info!("playgroup: {who} plays {group}: its model has no sequence of that name");
            }
            game.anims.set_playing(r.on, None);
            continue;
        };
        let desc = rig.clip(i).map(describe).unwrap_or_default();
        // The loop asked for already plays (a script asking every frame until it sees
        // it play): left as it is [design].
        let looping = rig.clip(i).is_some_and(|c| c.cycle_type != CLAMP);
        if looping && (rig.current == Some((r.group, i)) || rig.next == Some((r.group, i))) {
            st.stats.already += 1;
            continue;
        }
        // Flag 0: what plays finishes first (a group played once that has not ended, or
        // any loop, the idle included).
        let busy = playing.is_some_and(|p| !(p.cycle_type == CLAMP && p.time >= p.stop) && !p.held);
        if r.start == Start::Normal && busy {
            rig.next = Some((r.group, i));
            st.stats.queued += 1;
            if fresh {
                info!("playgroup: {who} plays {desc}, flag 0: after {} ends its cycle", playing.map(|p| p.name.as_str()).unwrap_or("-"));
            }
            continue;
        }
        start(&mut commands, root, &mut rig, r.group, i, StartAt::Start, mats.map(|m| m.into_inner()));
        started.insert(root);
        st.stats.started += 1;
        if fresh {
            info!("playgroup: {who} plays {desc}, flag {}", r.start.flag());
        }
    }

    // Ends, queued groups and sound keys.
    for (root, mut rig, playing, mats, at) in &mut rigs {
        let Some(p) = playing.filter(|_| !started.contains(&root)) else { continue };
        let cycles = cycle_count(p.time, p.start, p.stop, p.cycle_type);
        let t = p.sequence_time();
        let ended = p.cycle_type == CLAMP && p.time >= p.stop;
        let wrapped = rig.last.is_some_and(|(_, c)| c != cycles);
        // Sound keys of the group playing, passed since last frame.
        if let (Some((_, i)), Some((t0, c0))) = (rig.current, rig.last) {
            if let Some(clip) = rig.clip(i) {
                for (kt, k) in &clip.text_keys {
                    if passed(*kt, (t0, c0), (t, cycles), p.start) {
                        if let Some(key) = fnv_collide::anim::AnimSequence::sound_of(k) {
                            st.stats.sounds += 1;
                            let w = crate::world::bevy_to_game(at.translation()).to_array();
                            info!("playgroup: {} passes the sound key {key} of {} at {kt:.3} s", game.data.name(FormId(rig.form_id)), clip.name);
                            sounds.write(ObjectSound { at: w, key: key.to_string() });
                        }
                    }
                }
            }
        }
        rig.last = Some((t, cycles));
        if let Some((g, i)) = rig.next {
            if ended || wrapped {
                start(&mut commands, root, &mut rig, g, i, StartAt::Start, mats.map(|m| m.into_inner()));
                st.stats.started += 1;
                if let Some(c) = rig.clip(i) {
                    info!("playgroup: {} plays {}, its turn come", game.data.name(FormId(rig.form_id)), describe(c));
                }
                continue;
            }
        }
        if ended {
            if let Some((g, _)) = rig.current.take() {
                st.stats.ended += 1;
                let id = FormId(rig.form_id);
                if game.anims.playing(id) == Some(g) {
                    game.anims.set_playing(id, None);
                }
                debug!("playgroup: {} holds the end of its group", game.data.name(id));
            }
        }
    }
}

/// How many cycles a group has begun since its start: a clamped one holds its end and
/// begins no other (its keys are not passed again).
fn cycle_count(time: f32, start: f32, stop: f32, cycle_type: i64) -> i64 {
    if cycle_type == CLAMP {
        return 0;
    }
    ((time - start) / (stop - start).max(1e-6)).floor() as i64
}

/// Whether a key at sequence time `kt` was passed between the last frame's (time, cycle)
/// and this one's; a group just started has `(start - 1, 0)` as its last frame, so a key
/// at its start time counts.
fn passed(kt: f32, (t0, c0): (f32, i64), (t, c): (f32, i64), start: f32) -> bool {
    let from = if c0 != c { start - 1.0 } else { t0 };
    kt > from && kt <= t
}

/// `playgroup summary: …` at exit.
pub fn report(mut exit: MessageReader<AppExit>, st: Res<ObjectAnims>) {
    if exit.read().last().is_none() {
        return;
    }
    let s = &st.stats;
    let groups: Vec<String> = s.by_group.iter().map(|(g, n)| format!("{g} {n}")).collect();
    info!(
        "playgroup summary: {} asked ({}); {} started, {} waited for the playing one, {} loops already playing, {} played once and ended, {} for objects not drawn ({} shown when drawn), {} shown again when drawn again, {} without a sequence of that name; {} sound keys; {} for actors (actor group summary); {} resets (Reset3DState)",
        s.asked,
        if groups.is_empty() { "-".to_string() } else { groups.join(", ") },
        s.started,
        s.queued,
        s.already,
        s.ended,
        s.not_drawn,
        s.shown_when_drawn,
        s.restored,
        s.no_sequence,
        s.sounds,
        s.to_actors,
        s.resets
    );
}

/// The clip every drawn object plays and how far into it (seconds after its start), for
/// the collision that follows it (`nvf_play::phys::ObjectPoses`, issue #1396): the rigs
/// of scripted groups, idles, activators and containers alike.
pub fn publish_poses(poses: Option<ResMut<nvf_play::phys::ObjectPoses>>, rigs: Query<(&Playing, AnyOf<(&nvf_play::RefVisual, &GroupRig)>)>) {
    let Some(mut poses) = poses else { return };
    let poses = &mut poses.playing;
    poses.clear();
    for (p, (visual, group)) in &rigs {
        let Some(r) = group.map(|g| g.form_id).or(visual.map(|v| v.0)) else { continue };
        poses.insert(r, (p.name.clone(), p.sequence_time() - p.start));
    }
}

/// A model a script enabled at run time (`nvf_play::script_draw`) whose rig has named
/// sequences gets a [`GroupRig`] as render streaming's do (world.rs).
fn script_drawn(mut commands: Commands, mut models: ModelCtx, added: Query<(Entity, &ScriptDrawnRig), Added<ScriptDrawnRig>>) {
    for (e, d) in &added {
        let Some(model) = models.model(&d.path) else { continue };
        let mut rig = GroupRig::new(d.reference, model, &d.path, d.nodes.clone());
        rig.run_time = true;
        commands.entity(e).insert(rig);
    }
}

/// Added by nvforever with `--play`.
pub struct ObjectAnimsPlugin;

impl Plugin for ObjectAnimsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ObjectAnims>()
            .init_resource::<ActorGroups>()
            .add_message::<ObjectSound>()
            .add_systems(Update, (script_drawn, drive).chain().after(nvf_play::PlaySet::Scripts).before(crate::advance_animations))
            // Before physics poses the collision from it (`nvf_play::phys::pose_groups`):
            // what was drawn last frame.
            .add_systems(Update, publish_poses.before(nvf_play::PlaySet::Physics))
            // Actors' groups (actor_groups.rs): before their idles advance and they are posed.
            .add_systems(Update, crate::actor_groups::drive.after(drive).before(crate::idles::drive).before(crate::locomotion::walk_test))
            .add_systems(Last, (report, crate::actor_groups::report));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_keys_are_passed_once() {
        // The tripwire's Forward: its sound key 0.00001 s in, the clip 0.5 s, clamped.
        let (start, stop) = (0.0, 0.5);
        let first = (start - 1.0, 0);
        let t1 = 0.016;
        assert!(passed(0.00001, first, (t1, cycle_count(t1, start, stop, CLAMP)), start));
        // The bear trap's Forward (0.1 s, key at 0.033): passed on the frame that ends it,
        // and not again while its end is held.
        let (k, stop) = (0.0333, 0.1);
        assert!(passed(k, (0.025, 0), (0.05, cycle_count(0.05, start, stop, CLAMP)), start));
        assert!(!passed(k, (0.05, 0), (0.1, cycle_count(0.12, start, stop, CLAMP)), start));
        // A loop passes its keys again in its next cycle.
        assert_eq!(cycle_count(0.12, start, stop, 0), 1);
        assert!(passed(k, (0.09, 0), (0.04, 1), start));
    }
}
