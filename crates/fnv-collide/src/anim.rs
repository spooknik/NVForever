//! Node poses of a model under its embedded controller sequences, for collision that
//! follows an animation: a door's keyframed bodies move with the nodes its "Open" and
//! "Close" sequences drive (docs/spec-notes/doors.md). Headless: the renderer samples
//! the same sequences itself; the physics world and the navigation tools use this.
//!
//! Poses are in the mesh root's frame with the root's own transform left out, the frame
//! of [`crate::CollisionObject::node_transform`].

use crate::nif::Similarity;
use crate::shape::Iso;
use fnv_nif::anim::{sequences, TrackData, TransformTrack};
use fnv_nif::file::NifFile;
use fnv_nif::scene::{scene_objects, Decoded};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
struct Node {
    name: Option<String>,
    parent: Option<usize>,
    /// Bind (stored) local transform.
    bind: Similarity,
    /// The file's root node: its own transform is replaced by a placed reference's.
    root: bool,
}

/// One embedded controller sequence, bound to the model's nodes.
#[derive(Debug, Clone)]
pub struct AnimSequence {
    pub name: String,
    pub start: f32,
    pub stop: f32,
    pub cycle_type: i64,
    /// Text keys (sequence time, text).
    pub text_keys: Vec<(f32, String)>,
    /// (node index, track).
    tracks: Vec<(usize, Arc<TransformTrack>)>,
}

impl AnimSequence {
    pub fn length(&self) -> f32 {
        (self.stop - self.start).max(0.0)
    }

    /// Seconds from the start to the `end` text key, else the length. [observed: in all
    /// 658 door sequences with an `end` key, it is at the stop time; doors.md §1]
    pub fn end(&self) -> f32 {
        sequence_end(self.start, self.stop, &self.text_keys)
    }

    /// Text keys passed going from `from` to `to` seconds after the start (`from`
    /// excluded, `to` included; a key at the start counts when `from` is negative).
    pub fn keys_between(&self, from: f32, to: f32) -> impl Iterator<Item = &(f32, String)> + '_ {
        self.text_keys.iter().filter(move |(t, _)| {
            let s = t - self.start;
            s > from && s <= to
        })
    }

    /// The sound a text key names: `sound: <SOUN editor ID> [node]`, either case
    /// [observed: 586 such keys on door sequences; doors.md §1].
    pub fn sound_of(key: &str) -> Option<&str> {
        let (head, rest) = key.split_once(':')?;
        head.trim().eq_ignore_ascii_case("sound").then(|| rest.split_whitespace().next()).flatten()
    }

    /// Whether any track moves a node of the model.
    pub fn drives_nodes(&self) -> bool {
        !self.tracks.is_empty()
    }
}

/// Seconds from a sequence's start to its `end` text key, else its length
/// ([`AnimSequence::end`], for callers holding a sequence in another form).
pub fn sequence_end(start: f32, stop: f32, text_keys: &[(f32, String)]) -> f32 {
    // Non-finite times (a broken file) count as 0: the sequence ends at once.
    let length = Some(stop - start).filter(|l| l.is_finite()).unwrap_or(0.0).max(0.0);
    let end = text_keys.iter().find(|(_, k)| k.trim().eq_ignore_ascii_case("end")).map_or(length, |(t, _)| (t - start).clamp(0.0, length));
    if end.is_finite() {
        end
    } else {
        length
    }
}

/// Whether `time` has reached `end`; true when either is NaN.
fn ended(time: f32, end: f32) -> bool {
    !matches!(time.partial_cmp(&end), Some(std::cmp::Ordering::Less))
}

/// A sequence end time, with a non-finite one (a broken file) as 0.
fn finite_end(t: f32) -> f32 {
    if t.is_finite() {
        t.max(0.0)
    } else {
        0.0
    }
}

/// A model's node hierarchy and its sequences.
#[derive(Debug, Clone, Default)]
pub struct ModelAnim {
    nodes: Vec<Node>,
    /// Block index → node index.
    by_block: HashMap<usize, usize>,
    pub sequences: Vec<AnimSequence>,
}

/// A rotation quaternion (w, x, y, z) as a row-major 3×3 matrix (`p' = R p`, the
/// convention of [`Iso`] and of NIF transforms).
pub fn quat_to_rows([w, x, y, z]: [f32; 4]) -> [f32; 9] {
    [
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y - w * z),
        2.0 * (x * z + w * y),
        2.0 * (x * y + w * z),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z - w * x),
        2.0 * (x * z - w * y),
        2.0 * (y * z + w * x),
        1.0 - 2.0 * (x * x + y * y),
    ]
}

impl ModelAnim {
    pub fn new(file: &NifFile, dec: &Decoded) -> ModelAnim {
        let objs = scene_objects(file, dec);
        let root = file.footer.roots.first().and_then(|&r| usize::try_from(r).ok());
        let by_block: HashMap<usize, usize> = objs.iter().enumerate().map(|(k, o)| (o.block, k)).collect();
        let nodes = objs
            .iter()
            .map(|o| Node {
                name: o.name.clone(),
                parent: o.parent.and_then(|p| by_block.get(&p).copied()),
                bind: Similarity { iso: Iso { translation: o.transform.translation, rotation: o.transform.rotation }, scale: o.transform.scale },
                root: Some(o.block) == root,
            })
            .collect::<Vec<_>>();
        let by_name: HashMap<&str, usize> =
            nodes.iter().enumerate().filter_map(|(k, n)| Some((n.name.as_deref()?, k))).collect();
        let sequences = sequences(file, dec)
            .into_iter()
            .map(|s| {
                let tracks = s
                    .tracks
                    .into_iter()
                    .filter_map(|t| match (t.node_name, t.data) {
                        (Some(n), TrackData::Transform(tt)) => Some((*by_name.get(n.as_str())?, Arc::new(tt))),
                        _ => None,
                    })
                    .collect();
                AnimSequence {
                    name: s.name.unwrap_or_default(),
                    start: s.start_time,
                    stop: s.stop_time,
                    cycle_type: s.cycle_type,
                    text_keys: s.text_keys,
                    tracks,
                }
            })
            .collect();
        ModelAnim { nodes, by_block, sequences }
    }

    /// A model built in code (tests, tools): its nodes as (block, name, parent block, bind
    /// transform), the first the file's root; sequences added with
    /// [`ModelAnim::add_sequence`].
    pub fn from_nodes(nodes: impl IntoIterator<Item = (usize, Option<String>, Option<usize>, Similarity)>) -> ModelAnim {
        let list: Vec<_> = nodes.into_iter().collect();
        let by_block: HashMap<usize, usize> = list.iter().enumerate().map(|(k, n)| (n.0, k)).collect();
        let nodes = list
            .iter()
            .enumerate()
            .map(|(k, (_, name, parent, bind))| Node { name: name.clone(), parent: parent.and_then(|p| by_block.get(&p).copied()), bind: *bind, root: k == 0 })
            .collect();
        ModelAnim { nodes, by_block, sequences: Vec::new() }
    }

    /// Add a sequence from 0 to `stop` seconds moving the nodes at the given blocks (a
    /// track on a block that is not a node is dropped).
    pub fn add_sequence(&mut self, name: &str, stop: f32, cycle_type: i64, tracks: Vec<(usize, TransformTrack)>) {
        let tracks = tracks.into_iter().filter_map(|(b, t)| Some((*self.by_block.get(&b)?, Arc::new(t)))).collect();
        self.sequences.push(AnimSequence { name: name.to_string(), start: 0.0, stop, cycle_type, text_keys: Vec::new(), tracks });
    }

    /// The sequence called `name` (either case).
    pub fn sequence(&self, name: &str) -> Option<&AnimSequence> {
        self.sequences.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// A door's "Open" and "Close" sequences, when the model has both and they move
    /// nodes of it.
    pub fn door(&self) -> Option<(&AnimSequence, &AnimSequence)> {
        let (o, c) = (self.sequence("Open")?, self.sequence("Close")?);
        (o.drives_nodes() && c.drives_nodes()).then_some((o, c))
    }

    /// Whether some sequence moves the node at `block` or one of its ancestors (the
    /// file's root left out: its transform is the placement's): a track on it whose
    /// pose leaves the node's stored transform at some point of the sequence. A track
    /// that only holds the stored pose (an idle listing every node of the model) does
    /// not count. False when `block` is not a scene node.
    pub fn moves_block(&self, block: usize) -> bool {
        let Some(&first) = self.by_block.get(&block) else { return false };
        let mut k = first;
        for _ in 0..256 {
            let n = &self.nodes[k];
            if n.root && n.parent.is_none() {
                return false;
            }
            if self.sequences.iter().any(|s| self.track_moves(s, k)) {
                return true;
            }
            match n.parent {
                Some(p) => k = p,
                None => return false,
            }
        }
        false
    }

    /// The node at `block` in the mesh root's frame (the root's own transform left out)
    /// with `seq` at `time` seconds after its start (held at its ends), or in the bind
    /// pose without a sequence. `None` when `block` is not a scene node.
    pub fn node_to_root(&self, seq: Option<&AnimSequence>, time: f32, block: usize) -> Option<Similarity> {
        let mut k = *self.by_block.get(&block)?;
        let t = seq.map_or(0.0, |s| s.start + time.clamp(0.0, s.length()));
        let mut acc = Similarity::IDENTITY;
        let mut guard = 0;
        loop {
            let n = &self.nodes[k];
            if n.root && n.parent.is_none() {
                break;
            }
            acc = self.local(seq, t, k).compose(&acc);
            match n.parent {
                Some(p) if guard < 256 => k = p,
                _ => break,
            }
            guard += 1;
        }
        Some(acc)
    }

    /// Whether `seq` has a track on node `k` that takes it away from its stored
    /// transform, sampled at the track's key times and at even steps over the sequence
    /// (an engine choice: a hundredth of a unit, a thousandth of a rotation matrix entry
    /// or of the scale).
    fn track_moves(&self, seq: &AnimSequence, k: usize) -> bool {
        use fnv_nif::anim::Channel;
        let Some((_, track)) = seq.tracks.iter().find(|(n, _)| *n == k) else { return false };
        fn times<const N: usize>(c: &Channel<N>, out: &mut Vec<f32>) {
            if let Channel::Keys(k) = c {
                out.extend(k.keys.iter().map(|k| k.time));
            }
        }
        const STEPS: usize = 16;
        let mut ts: Vec<f32> = (0..=STEPS).map(|i| seq.start + (seq.stop - seq.start) * i as f32 / STEPS as f32).collect();
        times(&track.translation, &mut ts);
        times(&track.rotation, &mut ts);
        times(&track.scale, &mut ts);
        if let Some(e) = &track.euler {
            ts.extend(e.iter().flat_map(|k| k.keys.iter().map(|k| k.time)));
        }
        let bind = self.nodes[k].bind;
        ts.into_iter().any(|t| {
            let p = self.local(Some(seq), t, k);
            p.iso.translation.iter().zip(bind.iso.translation).any(|(a, b)| (a - b).abs() > 0.01)
                || p.iso.rotation.iter().zip(bind.iso.rotation).any(|(a, b)| (a - b).abs() > 1e-3)
                || (p.scale - bind.scale).abs() > 1e-3
        })
    }

    fn local(&self, seq: Option<&AnimSequence>, t: f32, k: usize) -> Similarity {
        let mut out = self.nodes[k].bind;
        let Some(track) = seq.and_then(|s| s.tracks.iter().find(|(n, _)| *n == k)).map(|(_, tt)| tt) else { return out };
        if let Some(v) = track.translation.sample(t) {
            out.iso.translation = v;
        }
        if let Some(q) = track.rotation_at(t) {
            out.iso.rotation = quat_to_rows(q);
        }
        if let Some([s]) = track.scale.sample(t) {
            out.scale = s;
        }
        out
    }
}

/// Where a door is in its open/close cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorPhase {
    Closed,
    Opening,
    Open,
    Closing,
}

/// A door's open state and the progress of its animation: the one clock the renderer,
/// the physics and the actors waiting at the door all follow (docs/design/navmesh.md
/// §6.1).
///
/// Engine choices, not observed game rules: a sequence counts as finished at its `end`
/// text key; a door told to close while opening (or the other way round) turns back
/// from the matching point of the other sequence (the same fraction from its end).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoorMotion {
    pub phase: DoorPhase,
    /// Seconds into the playing sequence (Opening/Closing); the held sequence's end
    /// otherwise.
    pub time: f32,
    pub open_end: f32,
    pub close_end: f32,
}

impl DoorMotion {
    /// At rest, open or closed; `open_end`/`close_end` from [`AnimSequence::end`].
    /// Non-finite ends count as 0 (the door snaps), so it never swings forever.
    pub fn new(open: bool, open_end: f32, close_end: f32) -> DoorMotion {
        let (open_end, close_end) = (finite_end(open_end), finite_end(close_end));
        let (phase, time) = if open { (DoorPhase::Open, open_end) } else { (DoorPhase::Closed, close_end) };
        DoorMotion { phase, time, open_end, close_end }
    }

    /// Fully open (the open sequence has ended).
    pub fn is_open(&self) -> bool {
        self.phase == DoorPhase::Open
    }

    /// Opening or closing. While it swings, a door's moving parts are not solid, so the
    /// leaf cannot hit or push an actor [engine choice, owner request 2026-10-09].
    pub fn swinging(&self) -> bool {
        matches!(self.phase, DoorPhase::Opening | DoorPhase::Closing)
    }

    /// Open or opening: where the door is headed.
    pub fn opens(&self) -> bool {
        matches!(self.phase, DoorPhase::Open | DoorPhase::Opening)
    }

    /// Head for open (`true`) or closed. Returns whether a sequence started.
    pub fn set_open(&mut self, open: bool) -> bool {
        let frac = |t: f32, end: f32| if end > 0.0 { (t / end).clamp(0.0, 1.0) } else { 1.0 };
        match (self.phase, open) {
            (DoorPhase::Closed, true) => {
                self.phase = DoorPhase::Opening;
                self.time = 0.0;
            }
            (DoorPhase::Open, false) => {
                self.phase = DoorPhase::Closing;
                self.time = 0.0;
            }
            (DoorPhase::Closing, true) => {
                self.time = (1.0 - frac(self.time, self.close_end)) * self.open_end;
                self.phase = DoorPhase::Opening;
            }
            (DoorPhase::Opening, false) => {
                self.time = (1.0 - frac(self.time, self.open_end)) * self.close_end;
                self.phase = DoorPhase::Closing;
            }
            _ => return false,
        }
        self.settle();
        true
    }

    /// Advance by `dt` seconds. Returns the sequence time before the step (to look up
    /// the text keys passed with [`AnimSequence::keys_between`]), `None` at rest.
    pub fn update(&mut self, dt: f32) -> Option<f32> {
        if !matches!(self.phase, DoorPhase::Opening | DoorPhase::Closing) {
            return None;
        }
        let before = self.time;
        self.time += dt.max(0.0);
        self.settle();
        Some(before)
    }

    /// A sequence ends at its end time; a NaN time or end (fields set by hand from bad
    /// data) ends it too, so the door is never left swinging.
    fn settle(&mut self) {
        match self.phase {
            DoorPhase::Opening if ended(self.time, self.open_end) => {
                self.phase = DoorPhase::Open;
                self.time = finite_end(self.open_end);
            }
            DoorPhase::Closing if ended(self.time, self.close_end) => {
                self.phase = DoorPhase::Closed;
                self.time = finite_end(self.close_end);
            }
            _ => {}
        }
    }

    /// The sequence that poses the door (true: "Open", false: "Close") and the time in it.
    pub fn pose(&self) -> (bool, f32) {
        (self.opens(), self.time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quaternion_rows_rotate_like_the_matrix() {
        // 90° about z: x → y.
        let h = std::f32::consts::FRAC_1_SQRT_2;
        let r = quat_to_rows([h, 0.0, 0.0, h]);
        let p = Iso { translation: [0.0; 3], rotation: r }.apply([1.0, 0.0, 0.0]);
        assert!((p[0]).abs() < 1e-6 && (p[1] - 1.0).abs() < 1e-6, "{p:?}");
    }

    fn rot_z(a: f32) -> [f32; 4] {
        [(a / 2.0).cos(), 0.0, 0.0, (a / 2.0).sin()]
    }

    #[test]
    fn node_to_root_follows_a_track_and_leaves_out_the_root() {
        use fnv_nif::anim::{Channel, Key, KeyKind, Keys};
        let sim = |t: [f32; 3]| Similarity { iso: Iso { translation: t, rotation: Iso::IDENTITY.rotation }, scale: 1.0 };
        // root (moved, left out) → hinge at x=10 → leaf 5 along the hinge's x.
        let nodes = vec![
            Node { name: Some("Root".into()), parent: None, bind: sim([100.0, 0.0, 0.0]), root: true },
            Node { name: Some("Hinge".into()), parent: Some(0), bind: sim([10.0, 0.0, 0.0]), root: false },
            Node { name: Some("Leaf".into()), parent: Some(1), bind: sim([5.0, 0.0, 0.0]), root: false },
        ];
        let key = |time: f32, value: [f32; 4]| Key { time, value, forward: [0.0; 4], backward: [0.0; 4], tbc: [0.0; 3] };
        let rotation = Channel::Keys(Keys { kind: KeyKind::Linear, keys: vec![key(0.0, rot_z(0.0)), key(1.0, rot_z(std::f32::consts::FRAC_PI_2))] });
        let track = TransformTrack { translation: Channel::Static(None), rotation, euler: None, scale: Channel::Static(None) };
        let seq = AnimSequence { name: "Open".into(), start: 0.0, stop: 1.0, cycle_type: 2, text_keys: vec![], tracks: vec![(1, Arc::new(track))] };
        let anim = ModelAnim { nodes, by_block: HashMap::from([(0, 0), (1, 1), (2, 2)]), sequences: vec![seq] };
        let s = anim.sequence("open");
        let at = |t: f32| anim.node_to_root(s, t, 2).unwrap().apply([0.0; 3]);
        let p0 = at(0.0);
        assert!((p0[0] - 15.0).abs() < 1e-4 && p0[1].abs() < 1e-4, "{p0:?}");
        let p1 = at(1.0);
        assert!((p1[0] - 10.0).abs() < 1e-4 && (p1[1] - 5.0).abs() < 1e-4, "{p1:?}");
        // Held at the end.
        assert_eq!(at(5.0), p1);
        // The hinge and the leaf under it move; the root (the placement's) does not count.
        assert!(anim.moves_block(1) && anim.moves_block(2));
        assert!(!anim.moves_block(0) && !anim.moves_block(9));
        // Bind pose without a sequence.
        let b = anim.node_to_root(None, 0.0, 2).unwrap().apply([0.0; 3]);
        assert!((b[0] - 15.0).abs() < 1e-4);
    }

    #[test]
    fn a_track_holding_the_stored_pose_moves_nothing() {
        use fnv_nif::anim::{Channel, Key, KeyKind, Keys};
        let sim = |t: [f32; 3]| Similarity { iso: Iso { translation: t, rotation: Iso::IDENTITY.rotation }, scale: 1.0 };
        let key = |time: f32, value: [f32; 3]| Key { time, value, forward: [0.0; 3], backward: [0.0; 3], tbc: [0.0; 3] };
        let held = |v: [f32; 3]| TransformTrack {
            translation: Channel::Keys(Keys { kind: KeyKind::Linear, keys: vec![key(0.0, v), key(2.0, v)] }),
            rotation: Channel::Static(Some([1.0, 0.0, 0.0, 0.0])),
            euler: None,
            scale: Channel::Static(Some([1.0])),
        };
        let mut anim = ModelAnim::from_nodes([
            (0, Some("Root".into()), None, sim([0.0; 3])),
            (1, Some("Gate".into()), Some(0), sim([10.0, 0.0, 0.0])),
            (2, Some("Post".into()), Some(0), sim([-10.0, 0.0, 0.0])),
            (3, Some("Leaf".into()), Some(2), sim([0.0, 5.0, 0.0])),
        ]);
        // An idle listing every node at its stored pose, keys that never change.
        anim.add_sequence("Idle", 2.0, 0, vec![(1, held([10.0, 0.0, 0.0])), (2, held([-10.0, 0.0, 0.0])), (3, held([0.0, 5.0, 0.0]))]);
        assert!(!anim.moves_block(1) && !anim.moves_block(2) && !anim.moves_block(3));
        // A group whose keys stay put but away from the stored pose does move the node.
        anim.add_sequence("Open", 2.0, 0, vec![(1, held([10.0, 0.0, 50.0]))]);
        assert!(anim.moves_block(1) && !anim.moves_block(3));
        // So does one whose keys change, and the node under it with it.
        let swing = TransformTrack {
            translation: Channel::Keys(Keys { kind: KeyKind::Linear, keys: vec![key(0.0, [-10.0, 0.0, 0.0]), key(2.0, [-10.0, 0.0, 80.0])] }),
            ..held([0.0; 3])
        };
        anim.add_sequence("Forward", 2.0, 0, vec![(2, swing)]);
        assert!(anim.moves_block(2) && anim.moves_block(3));
    }

    #[test]
    fn door_motion_opens_holds_and_turns_back() {
        let mut d = DoorMotion::new(false, 2.0, 1.0);
        assert_eq!(d.pose(), (false, 1.0));
        assert!(!d.is_open() && d.update(0.5).is_none());
        assert!(!d.swinging());
        assert!(d.set_open(true));
        assert!(d.swinging());
        assert!(!d.set_open(true), "already opening");
        assert_eq!(d.update(0.5), Some(0.0));
        assert_eq!((d.phase, d.time), (DoorPhase::Opening, 0.5));
        // Turned back a quarter of the way open: three quarters of the close remain done.
        d.set_open(false);
        assert_eq!(d.phase, DoorPhase::Closing);
        assert!((d.time - 0.75).abs() < 1e-6);
        d.update(1.0);
        assert_eq!(d.phase, DoorPhase::Closed);
        d.set_open(true);
        d.update(1.9);
        assert!(!d.is_open() && d.opens());
        d.update(0.1);
        assert!(d.is_open() && !d.swinging());
        assert_eq!(d.pose(), (true, 2.0));
    }

    #[test]
    fn non_finite_door_times_do_not_swing_forever() {
        let mut d = DoorMotion::new(false, f32::NAN, f32::INFINITY);
        assert_eq!((d.open_end, d.close_end), (0.0, 0.0));
        d.set_open(true);
        assert!(d.is_open() && !d.swinging());
        let mut d = DoorMotion { open_end: f32::NAN, ..DoorMotion::new(false, 1.0, 1.0) };
        d.set_open(true);
        assert!(d.is_open() && d.time == 0.0);
        let mut d = DoorMotion::new(false, 1.0, 1.0);
        d.set_open(true);
        d.time = f32::NAN;
        d.update(0.1);
        assert!(d.is_open() && d.time == 1.0);
        assert_eq!(sequence_end(0.0, f32::NAN, &[]), 0.0);
        assert_eq!(sequence_end(0.0, 2.0, &[(f32::NAN, "end".into())]), 2.0);
    }

    #[test]
    fn end_key_and_sounds() {
        let seq = AnimSequence {
            name: "Open".into(),
            start: 0.0,
            stop: 1.67,
            cycle_type: 2,
            text_keys: vec![(0.0, "start".into()), (0.00001, "sound: DRSMetalUtilitySmallOpen".into()), (1.5, "end".into())],
            tracks: vec![],
        };
        assert!((seq.end() - 1.5).abs() < 1e-6);
        let passed: Vec<_> = seq.keys_between(-1.0, 0.1).filter_map(|(_, k)| AnimSequence::sound_of(k)).collect();
        assert_eq!(passed, vec!["DRSMetalUtilitySmallOpen"]);
        assert_eq!(seq.keys_between(0.1, 1.0).count(), 0);
        assert_eq!(AnimSequence::sound_of("Sound: DRSVaultGearOpenSteam01 SoundNodeSteam"), Some("DRSVaultGearOpenSteam01"));
        assert_eq!(AnimSequence::sound_of("start"), None);
    }
}
