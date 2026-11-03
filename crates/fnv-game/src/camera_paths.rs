//! Camera paths (CPTH) and camera shots (CAMS): the original's VATS camera choices,
//! played by NVForever's VATS mode as kill cameras (docs/design/vats-mode.md §19,
//! docs/spec-notes/default-objects.md §3).
//!
//! - **CAMS** [documented: xEdit wbDefinitionsFNV.pas CAMS; GECK wiki CameraShot]: a model
//!   (a NIF holding an animated node over an `NiCamera`), `DATA` (the action that starts
//!   the shot: Shoot, Fly or Hit; where the camera is held and what it looks at:
//!   attacker, projectile or target; the flags; time multipliers for the player, the
//!   target and the world; the most and the least time the shot plays; the focus's share
//!   of the way between the actors) and `MNAM`, an image space modifier played during it.
//! - **CPTH** [documented: xEdit wbDefinitionsFNV.pas CPTH]: conditions, `ANAM` (the
//!   parent path and the previous sibling), `DATA` the camera zoom and `SNAM` the shots,
//!   repeated. The paths form a tree: a path's children are those naming it as parent,
//!   in the order of their previous-sibling links [documented: the field names; the
//!   order is how the editor lists them, hypothesis].
//!
//! How a path is chosen ([`Game::pick_camera_path`]) [hypothesis: the editor's tree read
//! as a decision tree; nothing documents the walk]: from the roots in order, the first
//! path whose conditions pass is entered; the first of its children chosen so wins, and
//! if none is, the path itself when it has shots; a passing path with nothing to give is
//! passed over for its next sibling. The conditions run on the player (subject) with the
//! shot's target as target, while GetVATSValue answers about the shot
//! (`crate::vats::VatsState`), and a GetDistance naming no reference measures to the
//! target [hypothesis: 22 of FalloutNV.esm's path conditions are GetDistance with a null
//! reference (observed); a distance to nothing would be meaningless].

use crate::cond::Condition;
use crate::game::Game;
use crate::vats::{CameraRoom, VatsOutcome, VatsShot, VatsState, MODE_PLAYBACK};
use fnv_script::vm::{ArgValue, Value};
use fnv_sim::FormId;
use serde_json::Value as Json;
use std::collections::{HashMap, HashSet};

/// What starts a shot (CAMS DATA "Action") [documented: xEdit; GECK wiki CameraShot
/// "Camera Action": when the command starts, when the projectile starts to fly, or when
/// the target takes the hit].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotAction {
    Shoot,
    Fly,
    Hit,
    Zoom,
}

/// Who a shot's camera is held by or looks at (CAMS DATA "Location", "Target")
/// [documented: xEdit].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotActor {
    Attacker,
    Projectile,
    Target,
}

/// CAMS DATA flags [documented: xEdit wbDefinitionsFNV.pas CAMS; GECK wiki CameraShot].
pub const POSITION_FOLLOWS_LOCATION: u32 = 0x01;
pub const ROTATION_FOLLOWS_TARGET: u32 = 0x02;
pub const DONT_FOLLOW_BONE: u32 = 0x04;
pub const FIRST_PERSON_CAMERA: u32 = 0x08;
pub const NO_TRACER: u32 = 0x10;
pub const START_AT_TIME_ZERO: u32 = 0x20;

/// One camera shot (CAMS).
#[derive(Debug, Clone, PartialEq)]
pub struct CameraShotDef {
    /// The model, as the record names it (under `meshes\`); none for a first-person shot.
    pub model: Option<String>,
    pub action: ShotAction,
    pub location: ShotActor,
    pub target: ShotActor,
    pub flags: u32,
    /// "Time Multiplier": the player's, the target's and the world's (Global) [documented:
    /// GECK wiki CameraShot: the global one "will adjust everything in the scene except
    /// for the VATS playback CameraShot"; the player's and the target's are relative to it].
    pub time_player: f32,
    pub time_target: f32,
    pub time_global: f32,
    /// The most and the least time the shot plays before the next [documented: GECK wiki
    /// CameraShot "Max Time", "Min Time"]; absent fields read 0.
    pub max_time: f32,
    pub min_time: f32,
    /// "Target % Between Actors" [documented: the focus is set that share of the way
    /// between the target and the actor].
    pub target_pct: f32,
    /// MNAM: an image space modifier played during the shot.
    pub imod: Option<FormId>,
}

impl CameraShotDef {
    pub fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }

    /// From the raw subrecords (DATA: four u32 then up to six f32 [documented: xEdit,
    /// "the last three fields may be absent"]). `None` without a DATA.
    pub fn from_raw(subs: &[fnv_esm::Subrecord], map: &dyn Fn(u32) -> Option<u32>) -> Option<CameraShotDef> {
        let data = &subs.iter().find(|s| &s.sig == b"DATA")?.data;
        let u = |i: usize| data.get(i * 4..i * 4 + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let f = |i: usize| u(i).map_or(0.0, f32::from_bits);
        let actor = |v: Option<u32>| match v {
            Some(1) => ShotActor::Projectile,
            Some(2) => ShotActor::Target,
            _ => ShotActor::Attacker,
        };
        let action = match u(0)? {
            1 => ShotAction::Fly,
            2 => ShotAction::Hit,
            3 => ShotAction::Zoom,
            _ => ShotAction::Shoot,
        };
        let model = subs.iter().find(|s| &s.sig == b"MODL").map(|s| zstring(&s.data)).filter(|m| !m.is_empty());
        let imod = subs.iter().find(|s| &s.sig == b"MNAM").and_then(|s| s.data.get(..4)).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).filter(|&l| l != 0).and_then(map).map(FormId);
        Some(CameraShotDef {
            model,
            action,
            location: actor(u(1)),
            target: actor(u(2)),
            flags: u(3).unwrap_or(0),
            time_player: f(4),
            time_target: f(5),
            time_global: f(6),
            max_time: f(7),
            min_time: f(8),
            target_pct: f(9),
            imod,
        })
    }
}

/// One camera path (CPTH).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CameraPathDef {
    pub parent: Option<FormId>,
    pub previous: Option<FormId>,
    /// DATA "Camera Zoom": 0 Default, 1 Disable, 2 Shot List [documented: xEdit]. Read,
    /// not used: VATS mode has no zoom.
    pub zoom: u8,
    pub shots: Vec<FormId>,
    pub conditions: Vec<Condition>,
}

impl CameraPathDef {
    /// From the subrecords as the schema decodes them (`val`; `effects.json` CPTH: ANAM
    /// two FormIDs, DATA the zoom enum, which `zoom` turns back into its code, SNAM a
    /// FormID); `cond` reads one CTDA.
    pub fn from_fields(
        subs: &[fnv_esm::Subrecord],
        val: &mut dyn FnMut(&fnv_esm::Subrecord) -> Option<Json>,
        zoom: &dyn Fn(&Json) -> Option<i64>,
        map: &dyn Fn(u32) -> Option<u32>,
        cond: &mut dyn FnMut(&fnv_esm::Subrecord) -> Option<Condition>,
    ) -> CameraPathDef {
        let form = |v: Option<&Json>| v.and_then(crate::data::hexid).filter(|&l| l != 0).and_then(map).map(FormId);
        let mut p = CameraPathDef::default();
        for s in subs {
            match &s.sig {
                b"ANAM" => {
                    let v = val(s);
                    p.parent = form(v.as_ref().and_then(|a| a.get(0)));
                    p.previous = form(v.as_ref().and_then(|a| a.get(1)));
                }
                b"DATA" => p.zoom = val(s).as_ref().and_then(zoom).unwrap_or(0) as u8,
                b"SNAM" => p.shots.extend(form(val(s).as_ref())),
                b"CTDA" => p.conditions.extend(cond(s)),
                _ => {}
            }
        }
        p
    }
}

/// The paths as a tree: the roots and each path's children, in order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CameraPathTree {
    pub roots: Vec<FormId>,
    pub children: HashMap<FormId, Vec<FormId>>,
}

impl CameraPathTree {
    /// A path whose parent is missing is a root. Siblings follow their previous-sibling
    /// links from those with none (or one outside the group), lowest FormID first; any
    /// left (a broken chain) follow in FormID order.
    pub fn build(paths: &HashMap<FormId, CameraPathDef>) -> CameraPathTree {
        let mut groups: HashMap<Option<FormId>, Vec<FormId>> = HashMap::new();
        for (&id, p) in paths {
            let parent = p.parent.filter(|q| paths.contains_key(q) && *q != id);
            groups.entry(parent).or_default().push(id);
        }
        let mut tree = CameraPathTree::default();
        for (parent, mut members) in groups {
            members.sort();
            let set: HashSet<FormId> = members.iter().copied().collect();
            let mut next: HashMap<FormId, Vec<FormId>> = HashMap::new();
            for &m in &members {
                if let Some(prev) = paths[&m].previous.filter(|p| set.contains(p) && *p != m) {
                    next.entry(prev).or_default().push(m);
                }
            }
            let mut order = Vec::with_capacity(members.len());
            let mut seen = HashSet::new();
            let starts: Vec<FormId> = members.iter().copied().filter(|m| paths[m].previous.is_none_or(|p| !set.contains(&p) || p == *m)).collect();
            for s in starts.into_iter().chain(members.iter().copied()) {
                let mut cur = Some(s);
                while let Some(c) = cur.filter(|c| seen.insert(*c)) {
                    order.push(c);
                    cur = next.get(&c).and_then(|n| n.iter().find(|x| !seen.contains(x)).copied());
                }
            }
            match parent {
                None => tree.roots = order,
                Some(p) => {
                    tree.children.insert(p, order);
                }
            }
        }
        tree
    }

    fn children_of(&self, p: FormId) -> &[FormId] {
        self.children.get(&p).map_or(&[], Vec::as_slice)
    }
}

/// GetDistance (condition index 1).
const GET_DISTANCE: u16 = 1;

/// A path's conditions as they run for `target`: a GetDistance naming no reference
/// measures to the target (module notes).
fn for_target(conds: &[Condition], target: FormId) -> std::borrow::Cow<'_, [Condition]> {
    let null = |c: &Condition| c.function == GET_DISTANCE && matches!(c.params.first(), None | Some(ArgValue::Value(Value::NONE)));
    if !conds.iter().any(null) {
        return std::borrow::Cow::Borrowed(conds);
    }
    std::borrow::Cow::Owned(
        conds
            .iter()
            .map(|c| {
                let mut c = c.clone();
                if null(&c) {
                    c.params = vec![ArgValue::Value(Value::Ref(target.0))];
                }
                c
            })
            .collect(),
    )
}

impl Game<'_> {
    /// How a shot that killed `target` came out, as far as the game knows (a kill
    /// camera's choice): fatal; critical when the killing hit was; a part exploded or
    /// severed when one is gone [design: crippling is not told apart from the kill].
    pub fn kill_outcome(&self, target: FormId) -> VatsOutcome {
        let e = self.extra.get(&target);
        let gone = |exploded: bool| e.is_some_and(|e| e.limbs_gone.values().any(|&x| x == exploded));
        VatsOutcome {
            critical: e.and_then(|e| e.last_hit).is_some_and(|h| h.critical),
            fatal: true,
            explode: gone(true),
            dismember: gone(false),
            cripple: false,
        }
    }

    /// The camera path the masters' tree chooses for `shot` with its `outcome` known and
    /// the room about the actors measured (module notes); `None` when no path with shots
    /// passes. GetVATSValue and the VATS area functions answer about it meanwhile.
    pub fn pick_camera_path(&mut self, shot: VatsShot, outcome: VatsOutcome, room: CameraRoom) -> Option<FormId> {
        let data = self.data;
        if data.camera_paths.is_empty() {
            return None;
        }
        let tree = CameraPathTree::build(&data.camera_paths);
        let was = self.vats;
        self.vats = VatsState { mode: MODE_PLAYBACK, shot: Some(shot), outcome: Some(outcome), room: Some(room) };
        let picked = self.walk_paths(&tree, &tree.roots.clone(), shot.target, 0);
        self.vats = was;
        picked
    }

    fn walk_paths(&mut self, tree: &CameraPathTree, list: &[FormId], target: FormId, depth: usize) -> Option<FormId> {
        let data = self.data;
        if depth > 32 {
            return None;
        }
        for &p in list {
            let Some(def) = data.camera_paths.get(&p) else { continue };
            let conds = for_target(&def.conditions, target);
            if !self.conditions_pass(&conds, Some(FormId::PLAYER_REF), Some(target), "CPTH") {
                continue;
            }
            if let Some(found) = self.walk_paths(tree, tree.children_of(p), target, depth + 1) {
                return Some(found);
            }
            if def.shots.iter().any(|s| data.camera_shots.contains_key(s)) {
                return Some(p);
            }
        }
        None
    }
}

/// The shots a kill camera plays from the hit, of a path's list: those started by the
/// hit, in order; with none, the last shot [design: VATS mode decides on a kill camera
/// once the shot has landed, so the shots before the hit (Shoot, Fly) have passed;
/// docs/design/vats-mode.md §19].
pub fn hit_shots(path: &CameraPathDef, shots: &HashMap<FormId, CameraShotDef>) -> Vec<FormId> {
    let known: Vec<FormId> = path.shots.iter().copied().filter(|s| shots.contains_key(s)).collect();
    let hit: Vec<FormId> = known.iter().copied().filter(|s| shots[s].action == ShotAction::Hit).collect();
    if hit.is_empty() {
        known.last().copied().into_iter().collect()
    } else {
        hit
    }
}

fn zstring(b: &[u8]) -> String {
    // In this run's code page (fnv_util::text, #1439).
    fnv_util::text::decode_z(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(parent: Option<u32>, previous: Option<u32>) -> CameraPathDef {
        CameraPathDef { parent: parent.map(FormId), previous: previous.map(FormId), ..Default::default() }
    }

    #[test]
    fn siblings_follow_their_previous_links() {
        // Roots 1 → 3 → 2 by links; 10 and 11 under 3, 11 first.
        let mut m = HashMap::new();
        m.insert(FormId(1), path(None, None));
        m.insert(FormId(2), path(None, Some(3)));
        m.insert(FormId(3), path(None, Some(1)));
        m.insert(FormId(10), path(Some(3), Some(11)));
        m.insert(FormId(11), path(Some(3), None));
        // A parent that does not exist makes a root; a broken link goes last.
        m.insert(FormId(20), path(Some(99), Some(98)));
        let t = CameraPathTree::build(&m);
        assert_eq!(t.roots, vec![FormId(1), FormId(3), FormId(2), FormId(20)]);
        assert_eq!(t.children_of(FormId(3)), &[FormId(11), FormId(10)]);
        assert!(t.children_of(FormId(1)).is_empty());
    }

    fn shot(action: u32) -> fnv_esm::Subrecord {
        let mut data = Vec::new();
        for v in [action, 2, 2, 7] {
            data.extend(v.to_le_bytes());
        }
        for v in [4.0f32, 3.0, 0.25, 3.0, 1.0, 0.0] {
            data.extend(v.to_le_bytes());
        }
        fnv_esm::Subrecord { sig: *b"DATA", data, extended: false }
    }

    #[test]
    fn a_shots_data_reads_in_order() {
        let subs = vec![
            fnv_esm::Subrecord { sig: *b"MODL", data: b"vatscameras\\simplefronthitl03.nif\0".to_vec(), extended: false },
            shot(2),
            fnv_esm::Subrecord { sig: *b"MNAM", data: 0x28FA1u32.to_le_bytes().to_vec(), extended: false },
        ];
        let s = CameraShotDef::from_raw(&subs, &|l| Some(l)).unwrap();
        assert_eq!(s.action, ShotAction::Hit);
        assert_eq!((s.location, s.target), (ShotActor::Target, ShotActor::Target));
        assert!(s.has(POSITION_FOLLOWS_LOCATION) && s.has(ROTATION_FOLLOWS_TARGET) && s.has(DONT_FOLLOW_BONE) && !s.has(FIRST_PERSON_CAMERA));
        assert_eq!((s.time_player, s.time_target, s.time_global, s.max_time, s.min_time), (4.0, 3.0, 0.25, 3.0, 1.0));
        assert_eq!(s.model.as_deref(), Some("vatscameras\\simplefronthitl03.nif"));
        assert_eq!(s.imod, Some(FormId(0x28FA1)));
        // A DATA without its last fields.
        let short = fnv_esm::Subrecord { sig: *b"DATA", data: shot(0).data[..28].to_vec(), extended: false };
        let s = CameraShotDef::from_raw(&[short], &|l| Some(l)).unwrap();
        assert_eq!((s.action, s.max_time, s.min_time), (ShotAction::Shoot, 0.0, 0.0));
    }

    #[test]
    fn a_paths_links_zoom_and_shots_read_through_the_schema() {
        let schema = fnv_esm::Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
        let mut ctx = fnv_esm::decode::DecodeCtx { schema: &schema, on_formid: None };
        let mut val = |s: &fnv_esm::Subrecord| fnv_esm::decode::decode_sub(b"CPTH", s, &mut ctx).value;
        let zoom = |v: &Json| crate::data::field_code(&schema, b"CPTH", b"DATA", None, v);
        let sub = |sig: &[u8; 4], data: Vec<u8>| fnv_esm::Subrecord { sig: *sig, data, extended: false };
        let words = |w: &[u32]| w.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>();
        // ANAM parent and previous sibling, DATA Shot List (2), two shots and a null one;
        // the FormIDs are mapped (here +0x100).
        let subs = vec![sub(b"ANAM", words(&[0x10, 0])), sub(b"DATA", vec![2]), sub(b"SNAM", words(&[0x20])), sub(b"SNAM", words(&[0])), sub(b"SNAM", words(&[0x21]))];
        let p = CameraPathDef::from_fields(&subs, &mut val, &zoom, &|l| Some(l + 0x100), &mut |_| None);
        assert_eq!((p.parent, p.previous, p.zoom), (Some(FormId(0x110)), None, 2));
        assert_eq!(p.shots, vec![FormId(0x120), FormId(0x121)]);
        // A zoom code past the schema's names keeps its number.
        let p = CameraPathDef::from_fields(&[sub(b"DATA", vec![1]), sub(b"ANAM", words(&[0, 0x11]))], &mut val, &zoom, &|l| Some(l), &mut |_| None);
        assert_eq!((p.parent, p.previous, p.zoom), (None, Some(FormId(0x11)), 1));
        let p = CameraPathDef::from_fields(&[sub(b"DATA", vec![7])], &mut val, &zoom, &|l| Some(l), &mut |_| None);
        assert_eq!(p.zoom, 7);
    }

    #[test]
    fn a_kill_camera_plays_the_hit_shots_or_the_last() {
        let def = |a| CameraShotDef::from_raw(&[shot(a)], &|l| Some(l)).unwrap();
        let shots: HashMap<FormId, CameraShotDef> = [(FormId(1), def(0)), (FormId(2), def(1)), (FormId(3), def(2)), (FormId(4), def(2))].into_iter().collect();
        let p = CameraPathDef { shots: vec![FormId(1), FormId(2), FormId(3), FormId(4)], ..Default::default() };
        assert_eq!(hit_shots(&p, &shots), vec![FormId(3), FormId(4)]);
        let p = CameraPathDef { shots: vec![FormId(1), FormId(2), FormId(77)], ..Default::default() };
        assert_eq!(hit_shots(&p, &shots), vec![FormId(2)]);
        assert!(hit_shots(&CameraPathDef::default(), &shots).is_empty());
    }
}
