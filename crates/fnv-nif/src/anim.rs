//! KF animation: controller sequences and transform tracks that can be sampled at any
//! time (docs/spec-notes/nif.md §6). Built on the schema-decoded blocks.

use crate::scene::Decoded;
use crate::value::{Struct, Value};

/// Interpolation of a key group (nif.xml KeyType).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Linear,
    /// Hermite with stored tangents: `backward` leaves the key, `forward` arrives at it.
    Quadratic,
    /// Kochanek–Bartels (tension, bias, continuity).
    Tbc,
    /// Rotation as three independent float key groups (Euler X, Y, Z).
    XyzRotation,
    /// Step: hold each key's value until the next key.
    Constant,
}

impl KeyKind {
    pub fn from_raw(v: i64) -> Option<KeyKind> {
        Some(match v {
            1 => KeyKind::Linear,
            2 => KeyKind::Quadratic,
            3 => KeyKind::Tbc,
            4 => KeyKind::XyzRotation,
            5 => KeyKind::Constant,
            _ => return None,
        })
    }
}

/// One key: time, value and (for quadratic keys) tangents; TBC parameters for TBC keys.
/// Despite the nif.xml names, `forward` is the tangent arriving at the key and
/// `backward` the one leaving it [observed: docs/spec-notes/nif.md §5].
#[derive(Debug, Clone, PartialEq)]
pub struct Key<const N: usize> {
    pub time: f32,
    pub value: [f32; N],
    pub forward: [f32; N],
    pub backward: [f32; N],
    pub tbc: [f32; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Keys<const N: usize> {
    pub kind: KeyKind,
    pub keys: Vec<Key<N>>,
}

fn arr<const N: usize>(v: Option<&Value>) -> Option<[f32; N]> {
    match v? {
        Value::Float(f) if N == 1 => Some([*f; N]),
        Value::Floats(f) => f.as_slice().try_into().ok(),
        _ => None,
    }
}

impl<const N: usize> Keys<N> {
    /// From a KeyGroup struct (num_keys, interpolation, keys).
    pub fn from_group(g: &Struct) -> Option<Keys<N>> {
        let kind = KeyKind::from_raw(g.int("interpolation").unwrap_or(1))?;
        let keys = g.list("keys").unwrap_or_default();
        Self::from_list(kind, keys)
    }

    pub fn from_list(kind: KeyKind, keys: &[Value]) -> Option<Keys<N>> {
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            let Value::Struct(k) = k else { return None };
            let value = arr::<N>(k.get("value"))?;
            let tbc = k
                .strukt("tbc")
                .map(|t| [t.float("tension").unwrap_or(0.0), t.float("bias").unwrap_or(0.0), t.float("continuity").unwrap_or(0.0)])
                .unwrap_or([0.0; 3]);
            out.push(Key {
                time: k.float("time").unwrap_or(0.0),
                value,
                forward: arr::<N>(k.get("forward")).unwrap_or([0.0; N]),
                backward: arr::<N>(k.get("backward")).unwrap_or([0.0; N]),
                tbc,
            });
        }
        Some(Keys { kind, keys: out })
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Value at time `t`, clamped to the first/last key. Quaternions (N = 4) are
    /// interpolated component-wise and normalised by the caller.
    pub fn sample(&self, t: f32) -> Option<[f32; N]> {
        let ks = &self.keys;
        let first = ks.first()?;
        if ks.len() == 1 || t <= first.time {
            return Some(first.value);
        }
        let last = ks.last().unwrap();
        if t >= last.time {
            return Some(last.value);
        }
        let i = ks.partition_point(|k| k.time <= t).saturating_sub(1).min(ks.len() - 2);
        let (a, b) = (&ks[i], &ks[i + 1]);
        let span = b.time - a.time;
        let u = if span > 0.0 { (t - a.time) / span } else { 0.0 };
        Some(match self.kind {
            KeyKind::Constant => a.value,
            KeyKind::Linear | KeyKind::XyzRotation => lerp(a.value, b.value, u),
            KeyKind::Quadratic => hermite(a.value, a.backward, b.value, b.forward, u),
            KeyKind::Tbc => {
                let prev = if i > 0 { &ks[i - 1] } else { a };
                let next = if i + 2 < ks.len() { &ks[i + 2] } else { b };
                let out_a = tbc_tangent(prev, a, b, true);
                let in_b = tbc_tangent(a, b, next, false);
                hermite(a.value, out_a, b.value, in_b, u)
            }
        })
    }
}

fn lerp<const N: usize>(a: [f32; N], b: [f32; N], u: f32) -> [f32; N] {
    std::array::from_fn(|k| a[k] + (b[k] - a[k]) * u)
}

/// Cubic Hermite between p0 and p1 with tangents m0 (leaving p0) and m1 (arriving at p1),
/// tangents expressed per key interval [documented: standard Hermite basis].
fn hermite<const N: usize>(p0: [f32; N], m0: [f32; N], p1: [f32; N], m1: [f32; N], u: f32) -> [f32; N] {
    let u2 = u * u;
    let u3 = u2 * u;
    let h00 = 2.0 * u3 - 3.0 * u2 + 1.0;
    let h10 = u3 - 2.0 * u2 + u;
    let h01 = -2.0 * u3 + 3.0 * u2;
    let h11 = u3 - u2;
    std::array::from_fn(|k| h00 * p0[k] + h10 * m0[k] + h01 * p1[k] + h11 * m1[k])
}

/// Kochanek–Bartels tangent at `cur` (outgoing when `outgoing`, else incoming)
/// [documented: Kochanek & Bartels 1984, "Interpolating splines with local tension,
/// continuity, and bias control"].
fn tbc_tangent<const N: usize>(prev: &Key<N>, cur: &Key<N>, next: &Key<N>, outgoing: bool) -> [f32; N] {
    let [t, b, c] = cur.tbc;
    let (fa, fb) = if outgoing {
        ((1.0 - t) * (1.0 + b) * (1.0 - c) / 2.0, (1.0 - t) * (1.0 - b) * (1.0 + c) / 2.0)
    } else {
        ((1.0 - t) * (1.0 + b) * (1.0 + c) / 2.0, (1.0 - t) * (1.0 - b) * (1.0 - c) / 2.0)
    };
    std::array::from_fn(|k| fa * (cur.value[k] - prev.value[k]) + fb * (next.value[k] - cur.value[k]))
}

/// A B-spline channel: `dim` components, control points already decompressed.
#[derive(Debug, Clone, PartialEq)]
pub struct BSplineChannel {
    pub dim: usize,
    /// `n` control points × `dim` floats.
    pub points: Vec<f32>,
}

/// Degree of the B-splines [hypothesis: cubic; nif.xml's description of the basis
/// ("frames plus degree minus one") does not fix the value; checked indirectly in
/// docs/spec-notes/nif.md §6].
pub const BSPLINE_DEGREE: usize = 3;

impl BSplineChannel {
    pub fn count(&self) -> usize {
        self.points.len() / self.dim.max(1)
    }

    /// Evaluate the open uniform B-spline at normalised time `u` in [0, 1].
    pub fn eval(&self, u: f32) -> Vec<f32> {
        (0..self.dim).map(|c| self.eval_component(u, c)).collect()
    }

    /// [`Self::eval`] for an `N`-component channel without allocating (animation runs
    /// every frame); `None` when the channel does not have `N` components.
    pub fn eval_array<const N: usize>(&self, u: f32) -> Option<[f32; N]> {
        (self.dim == N).then(|| std::array::from_fn(|k| self.eval_component(u, k)))
    }

    /// Component `c` of the spline at `u`: de Boor's algorithm on that component.
    fn eval_component(&self, u: f32, c: usize) -> f32 {
        let n = self.count();
        if n == 0 {
            return 0.0;
        }
        let d = BSPLINE_DEGREE.min(n - 1);
        if d == 0 {
            return self.points[c];
        }
        // Open uniform knot vector: d+1 zeros, uniform interior knots, d+1 maxima.
        let max_knot = (n - d) as f32;
        let knot = |i: usize| -> f32 {
            if i <= d {
                0.0
            } else if i >= n {
                max_knot
            } else {
                (i - d) as f32
            }
        };
        let x = u.clamp(0.0, 1.0) * max_knot;
        // Span index s with knot(s) <= x < knot(s+1), clamped at the end.
        let mut s = d;
        while s < n - 1 && knot(s + 1) <= x {
            s += 1;
        }
        // De Boor's algorithm [documented: de Boor 1972].
        let mut pts = [0.0f32; BSPLINE_DEGREE + 1];
        for (j, p) in pts.iter_mut().enumerate().take(d + 1) {
            *p = self.points[(s - d + j) * self.dim + c];
        }
        for r in 1..=d {
            for j in (r..=d).rev() {
                let i = s - d + j;
                let denom = knot(i + d + 1 - r) - knot(i);
                let alpha = if denom > 0.0 { (x - knot(i)) / denom } else { 0.0 };
                pts[j] = (1.0 - alpha) * pts[j - 1] + alpha * pts[j];
            }
        }
        pts[d]
    }
}

/// One animated property of a transform.
#[derive(Debug, Clone, PartialEq)]
pub enum Channel<const N: usize> {
    /// Not animated by this track: use the interpolator's static pose (if any).
    Static(Option<[f32; N]>),
    Keys(Keys<N>),
    BSpline { start: f32, stop: f32, spline: BSplineChannel },
    /// A place on a curve (`NiPathInterpolator`).
    Path(Box<PathChannel>),
}

/// A `NiPathInterpolator`'s place: a curve (`NiPosData` keys) and how far along it the
/// node is over time (`NiFloatData`, 0 at the curve's first key and 1 at its last)
/// [documented: nif.xml NiPathInterpolator "Path Data", "Percent Data"; PathFlags].
/// With the flag `constant_velocity` the share is of the curve's length, else of its
/// key times [hypothesis: the flag's name; `fxdustwhirlwind01.nif` sets it, with path
/// keys at 0, 0.5 and 1 whose two spans differ in length]. The flags `follow` and
/// `bank` (turning the node along the curve) are not read: the three ambient files with
/// a path set neither [observed, docs/spec-notes/particles.md §6.8].
#[derive(Debug, Clone, PartialEq)]
pub struct PathChannel {
    pub path: Keys<3>,
    pub percent: Keys<1>,
    /// With constant velocity: the path's key time at each [`PATH_LENGTH_STEPS`]th of
    /// its length.
    pub by_length: Option<Vec<f32>>,
}

/// How finely a constant-velocity path is measured.
pub const PATH_LENGTH_STEPS: usize = 64;

impl PathChannel {
    pub fn new(path: Keys<3>, percent: Keys<1>, constant_velocity: bool) -> PathChannel {
        let by_length = constant_velocity.then(|| Self::length_table(&path)).flatten();
        PathChannel { path, percent, by_length }
    }

    /// Key times at equal steps of the curve's length (`None` for a curve of no length).
    fn length_table(path: &Keys<3>) -> Option<Vec<f32>> {
        let (t0, t1) = (path.keys.first()?.time, path.keys.last()?.time);
        const SAMPLES: usize = 512;
        let at = |k: usize| t0 + (t1 - t0) * k as f32 / SAMPLES as f32;
        let mut lengths = vec![0.0f32; SAMPLES + 1];
        let mut prev = path.sample(t0)?;
        for k in 1..=SAMPLES {
            let p = path.sample(at(k))?;
            let d = ((p[0] - prev[0]).powi(2) + (p[1] - prev[1]).powi(2) + (p[2] - prev[2]).powi(2)).sqrt();
            lengths[k] = lengths[k - 1] + d;
            prev = p;
        }
        let total = lengths[SAMPLES];
        if total <= 0.0 || !total.is_finite() {
            return None;
        }
        let mut table = Vec::with_capacity(PATH_LENGTH_STEPS + 1);
        let mut k = 0;
        for step in 0..=PATH_LENGTH_STEPS {
            let want = total * step as f32 / PATH_LENGTH_STEPS as f32;
            while k < SAMPLES && lengths[k + 1] < want {
                k += 1;
            }
            let k1 = (k + 1).min(SAMPLES);
            let span = lengths[k1] - lengths[k];
            let u = if span > 0.0 { ((want - lengths[k]) / span).clamp(0.0, 1.0) } else { 0.0 };
            table.push(at(k) + (at(k1) - at(k)) * u);
        }
        Some(table)
    }

    /// The place at time `t`.
    pub fn sample(&self, t: f32) -> Option<[f32; 3]> {
        let share = self.percent.sample(t)?[0];
        let share = if share.is_finite() { share.clamp(0.0, 1.0) } else { 0.0 };
        let u = match &self.by_length {
            Some(table) => {
                let x = share * (table.len() - 1) as f32;
                let i = (x.floor() as usize).min(table.len() - 2);
                table[i] + (table[i + 1] - table[i]) * (x - i as f32)
            }
            None => {
                let (t0, t1) = (self.path.keys.first()?.time, self.path.keys.last()?.time);
                t0 + (t1 - t0) * share
            }
        };
        self.path.sample(u)
    }
}

/// Whether every component is a finite number.
fn finite<const N: usize>(v: &[f32; N]) -> bool {
    v.iter().all(|x| x.is_finite())
}

impl<const N: usize> Channel<N> {
    /// The value at `t`; `None` where it is not a finite number (a broken file's keys,
    /// or finite keys whose interpolation overflows), so the caller keeps its rest value
    /// rather than passing NaN on to a transform.
    pub fn sample(&self, t: f32) -> Option<[f32; N]> {
        self.sample_raw(t).filter(finite)
    }

    fn sample_raw(&self, t: f32) -> Option<[f32; N]> {
        match self {
            Channel::Static(v) => *v,
            Channel::Keys(k) => k.sample(t),
            Channel::BSpline { start, stop, spline } => {
                let span = stop - start;
                let u = if span > 0.0 { (t - start) / span } else { 0.0 };
                spline.eval_array(u)
            }
            Channel::Path(p) => {
                let v = p.sample(t)?;
                Some(std::array::from_fn(|k| v.get(k).copied().unwrap_or(0.0)))
            }
        }
    }

    pub fn is_animated(&self) -> bool {
        !matches!(self, Channel::Static(_))
    }
}

/// Translation, rotation (quaternion w, x, y, z) and uniform scale over time.
#[derive(Debug, Clone, PartialEq)]
pub struct TransformTrack {
    pub translation: Channel<3>,
    pub rotation: Channel<4>,
    /// Euler XYZ rotation keys (radians), used instead of `rotation` when present.
    pub euler: Option<[Keys<1>; 3]>,
    pub scale: Channel<1>,
}

impl TransformTrack {
    /// Rotation at `t` as a normalised quaternion (w, x, y, z); `None` where it is not
    /// a finite number.
    pub fn rotation_at(&self, t: f32) -> Option<[f32; 4]> {
        if let Some(e) = &self.euler {
            let a: [f32; 3] = std::array::from_fn(|k| e[k].sample(t).map_or(0.0, |v| v[0]));
            return Some(euler_xyz_to_quat(a)).filter(finite);
        }
        let q = match &self.rotation {
            Channel::Keys(k) => sample_quat_keys(k, t)?,
            other => other.sample(t)?,
        };
        Some(normalize(q)).filter(finite)
    }
}

/// `q` at unit length; the identity for a zero or overflowing one. Divided by its largest
/// component first, so that the squares of large but finite keys do not overflow.
fn normalize(q: [f32; 4]) -> [f32; 4] {
    let m = q.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if !(m > 0.0 && m.is_finite()) {
        return [1.0, 0.0, 0.0, 0.0];
    }
    let q = q.map(|x| x / m);
    let len = (q.iter().map(|x| x * x).sum::<f32>()).sqrt();
    q.map(|x| x / len)
}

/// Spherical linear interpolation along the shorter arc [documented: Shoemake 1985,
/// "Animating rotation with quaternion curves"].
pub fn slerp(a: [f32; 4], b: [f32; 4], u: f32) -> [f32; 4] {
    let mut dot: f32 = a.iter().zip(&b).map(|(x, y)| x * y).sum();
    let b = if dot < 0.0 {
        dot = -dot;
        b.map(|x| -x)
    } else {
        b
    };
    if dot > 0.9995 {
        return normalize(std::array::from_fn(|k| a[k] + (b[k] - a[k]) * u));
    }
    let theta = dot.clamp(-1.0, 1.0).acos();
    let s = theta.sin();
    let (wa, wb) = (((1.0 - u) * theta).sin() / s, (u * theta).sin() / s);
    std::array::from_fn(|k| wa * a[k] + wb * b[k])
}

/// Quaternion keys: step for constant keys, slerp otherwise. Quadratic and TBC rotation
/// keys are approximated by slerp between neighbouring keys [hypothesis: the original
/// uses a smoother quaternion spline; 180 of 213,701 transform tracks use TBC rotation
/// keys and none use quadratic ones, observed].
pub fn sample_quat_keys(k: &Keys<4>, t: f32) -> Option<[f32; 4]> {
    let ks = &k.keys;
    let first = ks.first()?;
    if ks.len() == 1 || t <= first.time {
        return Some(first.value);
    }
    let last = ks.last().unwrap();
    if t >= last.time {
        return Some(last.value);
    }
    let i = ks.partition_point(|x| x.time <= t).saturating_sub(1).min(ks.len() - 2);
    let (a, b) = (&ks[i], &ks[i + 1]);
    if k.kind == KeyKind::Constant {
        return Some(a.value);
    }
    let span = b.time - a.time;
    let u = if span > 0.0 { (t - a.time) / span } else { 0.0 };
    Some(slerp(a.value, b.value, u))
}

/// Rotation applying X, then Y, then Z about fixed axes [hypothesis: the order used by
/// nif.xml XYZ_ROTATION_KEY data; only a handful of files use it].
pub fn euler_xyz_to_quat(a: [f32; 3]) -> [f32; 4] {
    let (sx, cx) = (a[0] / 2.0).sin_cos();
    let (sy, cy) = (a[1] / 2.0).sin_cos();
    let (sz, cz) = (a[2] / 2.0).sin_cos();
    // q = qz * qy * qx
    [
        cz * cy * cx + sz * sy * sx,
        cz * cy * sx - sz * sy * cx,
        cz * sy * cx + sz * cy * sx,
        sz * cy * cx - cz * sy * sx,
    ]
}

/// What a controlled block drives.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackData {
    Transform(TransformTrack),
    /// An interpolator type the animation layer does not evaluate yet (its type name).
    Other(String),
    /// No interpolator (null reference).
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub node_name: Option<String>,
    pub controller_type: Option<String>,
    pub priority: Option<i64>,
    pub data: TrackData,
    /// The on/off values of a `NiBoolInterpolator` (or `NiBoolTimelineInterpolator`)
    /// track (`data` is then `Other`): under a `NiVisController`, whether the node is
    /// shown.
    pub bools: Option<BoolTrack>,
    /// The controlled block's controller ID: which member of the target the controller
    /// drives, e.g. `SELF_ILLUM` under a `NiMaterialColorController`, `0-0-TT_TRANSLATE_U`
    /// under a `NiTextureTransformController` [observed: docs/spec-notes/activators.md §2].
    pub controller_id: Option<String>,
    /// The values of a `NiFloatInterpolator` or `NiPoint3Interpolator` track (`data` is
    /// then `Other`): a colour, a texture offset, a multiplier.
    pub values: Option<ValueTrack>,
    /// The controlled block's interpolator ID: which of the controller's interpolators
    /// the track feeds; under a `NiGeomMorpherController`, the morph's frame name
    /// [observed: `utilitypolewsmall01nv.nif`'s Idle names `Base`,
    /// `UtilityPoleWSmall01WireM01` and `…M02`, its morphs' frame names;
    /// docs/spec-notes/vertex-morphs.md §1].
    pub interpolator_id: Option<String>,
}

/// A `NiFloatInterpolator` or `NiPoint3Interpolator` evaluated: its keys (`NiFloatData`,
/// `NiPosData`), else its own value; or a compressed B-spline one.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueTrack {
    Float(Channel<1>),
    Point3(Channel<3>),
}

impl ValueTrack {
    /// The value at `t`; a float track gives its value three times.
    pub fn at(&self, t: f32) -> Option<[f32; 3]> {
        match self {
            ValueTrack::Float(c) => c.sample(t).map(|[v]| [v, v, v]),
            ValueTrack::Point3(c) => c.sample(t),
        }
    }
}

/// The float or point3 interpolator at block `idx`, by its type name.
pub fn value_track(dec: &Decoded, idx: i32, type_name: &str) -> Option<ValueTrack> {
    let s = dec.get(idx)?;
    // nif.xml marks an unset value with -FLT_MAX [documented].
    let valid = |x: f32| x > -3.0e38;
    let group = s.block_ref("data").and_then(|r| dec.get(r)).and_then(|d| d.strukt("data"));
    match type_name {
        "NiFloatInterpolator" => {
            let keys = group.and_then(Keys::<1>::from_group).filter(|k| !k.is_empty());
            Some(ValueTrack::Float(match keys {
                Some(k) => Channel::Keys(k),
                None => Channel::Static(s.float("value").filter(|v| valid(*v)).map(|v| [v])),
            }))
        }
        "NiPoint3Interpolator" => {
            let keys = group.and_then(Keys::<3>::from_group).filter(|k| !k.is_empty());
            Some(ValueTrack::Point3(match keys {
                Some(k) => Channel::Keys(k),
                None => Channel::Static(s.floats("value").and_then(|v| <[f32; 3]>::try_from(v).ok()).filter(|v| v.iter().all(|x| valid(*x)))),
            }))
        }
        // Compressed B-spline values: the Securitron screens' glow in `mtidle.kf` and
        // `talking.kf` [observed; documented: nif.xml NiBSplineCompPoint3Interpolator
        // "position offset" and "half range", NiBSplineCompFloatInterpolator "float
        // offset" and "half range", decompressed as the transform ones are].
        "NiBSplineCompPoint3Interpolator" => {
            let (start, stop) = (s.float("start_time")?, s.float("stop_time")?);
            Some(ValueTrack::Point3(match bspline_channel(dec, s, "handle", 3, Some(("position_offset", "position_half_range"))) {
                Some(spline) => Channel::BSpline { start, stop, spline },
                None => Channel::Static(s.floats("value").and_then(|v| <[f32; 3]>::try_from(v).ok()).filter(|v| v.iter().all(|x| valid(*x)))),
            }))
        }
        "NiBSplineCompFloatInterpolator" => {
            let (start, stop) = (s.float("start_time")?, s.float("stop_time")?);
            Some(ValueTrack::Float(match bspline_channel(dec, s, "handle", 1, Some(("float_offset", "float_half_range"))) {
                Some(spline) => Channel::BSpline { start, stop, spline },
                None => Channel::Static(s.float("value").filter(|v| valid(*v)).map(|v| [v])),
            }))
        }
        _ => None,
    }
}

/// A `NiBoolInterpolator` evaluated: its own value, or the step keys of its `NiBoolData`
/// [observed: `2hhholster.kf` hides weapon parts with a keyless value, `2hhequip.kf`
/// with one key per frame; docs/spec-notes/animation.md §9.7].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoolTrack {
    /// The interpolator's own value (the schema's `value` field; 2 there stands for none).
    pub value: Option<bool>,
    /// (time, value) keys, in time order.
    pub keys: Vec<(f32, bool)>,
}

impl BoolTrack {
    /// The value at time `t`: that of the last key at or before `t` (of the first key
    /// before them all) [hypothesis: keys step, they do not blend]; without keys, the
    /// interpolator's own value.
    pub fn at(&self, t: f32) -> Option<bool> {
        match self.keys.iter().rev().find(|(time, _)| *time <= t + 1e-4).or(self.keys.first()) {
            Some((_, v)) => Some(*v),
            None => self.value,
        }
    }
}

/// The `NiBoolInterpolator` at block `idx`.
pub fn bool_track(dec: &Decoded, idx: i32) -> Option<BoolTrack> {
    let s = dec.get(idx)?;
    let as_bool = |v: &Value| match v {
        Value::Bool(b) => Some(*b),
        Value::Int(0) => Some(false),
        Value::Int(1) => Some(true),
        _ => None,
    };
    let value = s.get("value").and_then(as_bool);
    let mut keys = Vec::new();
    if let Some(group) = s.block_ref("data").and_then(|r| dec.get(r)).and_then(|d| d.strukt("data")) {
        for k in group.list("keys").unwrap_or_default() {
            let Value::Struct(k) = k else { continue };
            if let (Some(t), Some(v)) = (k.float("time"), k.get("value").and_then(as_bool)) {
                keys.push((t, v));
            }
        }
    }
    Some(BoolTrack { value, keys })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    pub block: usize,
    pub name: Option<String>,
    pub start_time: f32,
    pub stop_time: f32,
    pub frequency: f32,
    /// nif.xml CycleType: 0 loop, 1 reverse, 2 clamp.
    pub cycle_type: i64,
    pub accum_root_name: Option<String>,
    pub text_keys: Vec<(f32, String)>,
    pub tracks: Vec<Track>,
}

/// Text of a ControlledBlock string: the string itself (20.1.0.1+) or a string-palette
/// offset (older files).
fn cb_string(dec: &Decoded, cb: &Struct, name: &str) -> Option<String> {
    if let Some(t) = cb.text(name) {
        return Some(t.to_string());
    }
    let off = cb.int(&format!("{name}_offset"))?;
    if off < 0 || off == 0xFFFF_FFFF {
        return None;
    }
    let palette = dec.get(cb.block_ref("string_palette")?)?;
    // Offsets count bytes of the on-disk (Windows-1252) palette.
    let bytes = fnv_util::text::encode(palette.text("palette")?)?;
    let start = off as usize;
    if start > bytes.len() {
        return None;
    }
    let end = bytes[start..].iter().position(|&b| b == 0).map_or(bytes.len(), |p| start + p);
    Some(fnv_util::text::decode(&bytes[start..end]))
}

/// Static pose parts of an NiQuatTransform: translation, rotation, scale.
type StaticPose = (Option<[f32; 3]>, Option<[f32; 4]>, Option<[f32; 1]>);

fn quat_transform(s: Option<&Struct>) -> StaticPose {
    let Some(s) = s else { return (None, None, None) };
    // nif.xml marks unused parts of an NiQuatTransform with -FLT_MAX [documented].
    let valid = |x: f32| x > -3.0e38;
    let t = s.floats("translation").and_then(|v| <[f32; 3]>::try_from(v).ok()).filter(|v| v.iter().all(|x| valid(*x)));
    let r = s.floats("rotation").and_then(|v| <[f32; 4]>::try_from(v).ok()).filter(|v| v.iter().all(|x| valid(*x)));
    let sc = s.float("scale").filter(|x| valid(*x)).map(|x| [x]);
    (t, r, sc)
}

fn bspline_channel(
    dec: &Decoded,
    it: &Struct,
    handle: &str,
    dim: usize,
    compact: Option<(&str, &str)>,
) -> Option<BSplineChannel> {
    let h = it.int(handle)?;
    if h == 0xFFFF || h < 0 {
        return None;
    }
    let data = dec.get(it.block_ref("spline_data")?)?;
    // A broken file's negative or huge count gives no channel.
    let n = usize::try_from(dec.get(it.block_ref("basis_data")?)?.int("num_control_points")?).ok()?;
    let h = h as usize;
    let end = n.checked_mul(dim)?.checked_add(h)?;
    let points: Vec<f32> = match compact {
        Some((off, half)) => {
            let (o, r) = (it.float(off)?, it.float(half)?);
            let cps = data.ints("compact_control_points")?;
            cps.get(h..end)?.iter().map(|&c| o + (c as f32 / 32767.0) * r).collect()
        }
        None => data.floats("float_control_points")?.get(h..end)?.to_vec(),
    };
    Some(BSplineChannel { dim, points })
}

/// Build the transform track for an interpolator block, if it is a transform type.
pub fn transform_track(dec: &Decoded, idx: i32, type_name: &str) -> Option<TransformTrack> {
    let it = dec.get(idx)?;
    let (t0, r0, s0) = quat_transform(it.strukt("transform"));
    match type_name {
        "NiTransformInterpolator" => {
            let mut track = TransformTrack {
                translation: Channel::Static(t0),
                rotation: Channel::Static(r0),
                euler: None,
                scale: Channel::Static(s0),
            };
            if let Some(data) = it.block_ref("data").and_then(|d| dec.get(d)) {
                if let Some(g) = data.strukt("translations").and_then(Keys::<3>::from_group).filter(|k| !k.is_empty()) {
                    track.translation = Channel::Keys(g);
                }
                if let Some(g) = data.strukt("scales").and_then(Keys::<1>::from_group).filter(|k| !k.is_empty()) {
                    track.scale = Channel::Keys(g);
                }
                let kind = data.int("rotation_type").and_then(KeyKind::from_raw);
                match kind {
                    Some(KeyKind::XyzRotation) => {
                        if let Some(groups) = data.list("xyz_rotations") {
                            let g: Vec<Keys<1>> = groups
                                .iter()
                                .filter_map(|g| match g {
                                    Value::Struct(s) => Keys::<1>::from_group(s),
                                    _ => None,
                                })
                                .collect();
                            if let Ok(g) = <[Keys<1>; 3]>::try_from(g) {
                                track.euler = Some(g);
                            }
                        }
                    }
                    Some(k) => {
                        if let Some(keys) = data.list("quaternion_keys").and_then(|l| Keys::<4>::from_list(k, l)) {
                            if !keys.is_empty() {
                                track.rotation = Channel::Keys(keys);
                            }
                        }
                    }
                    None => {}
                }
            }
            Some(track)
        }
        // A node aimed at another (a camera at its target): its place comes from its
        // translation interpolator (an `NiPoint3Interpolator`); the aim is left to the
        // caller, who knows the target node (`look_at_name`) [documented: nif.xml
        // NiLookAtInterpolator; observed: `nv_roulette-table.nif`'s `Camera01` in its
        // `Spin_1`–`Spin_3` sequences, docs/spec-notes/roulette.md §6].
        "NiLookAtInterpolator" => {
            let place = it.block_ref("translation_interpolator").and_then(|r| value_track(dec, r, "NiPoint3Interpolator"));
            let translation = match place {
                Some(ValueTrack::Point3(c)) => c,
                _ => Channel::Static(t0),
            };
            Some(TransformTrack { translation, rotation: Channel::Static(None), euler: None, scale: Channel::Static(None) })
        }
        // A node moved along a curve: the dust devils' and drifting dust's movers
        // (docs/spec-notes/particles.md §6.8). Its rotation and scale are the node's own.
        "NiPathInterpolator" => {
            let keys3 = |field: &str| it.block_ref(field).and_then(|r| dec.get(r)).and_then(|d| d.strukt("data")).and_then(Keys::<3>::from_group).filter(|k| !k.is_empty());
            let path = keys3("path_data")?;
            let percent = it.block_ref("percent_data").and_then(|r| dec.get(r)).and_then(|d| d.strukt("data")).and_then(Keys::<1>::from_group).filter(|k| !k.is_empty())?;
            let constant_velocity = it.int("flags").is_some_and(|f| f & (1 << 4) != 0);
            Some(TransformTrack {
                translation: Channel::Path(Box::new(PathChannel::new(path, percent, constant_velocity))),
                rotation: Channel::Static(None),
                euler: None,
                scale: Channel::Static(None),
            })
        }
        "NiBSplineCompTransformInterpolator" | "NiBSplineTransformInterpolator" => {
            let comp = type_name == "NiBSplineCompTransformInterpolator";
            let (start, stop) = (it.float("start_time")?, it.float("stop_time")?);
            let ch = |handle: &str, dim: usize, off: &'static str, half: &'static str| {
                bspline_channel(dec, it, handle, dim, comp.then_some((off, half)))
            };
            let wrap3 = |c: Option<BSplineChannel>, s: Option<[f32; 3]>| match c {
                Some(spline) => Channel::BSpline { start, stop, spline },
                None => Channel::Static(s),
            };
            Some(TransformTrack {
                translation: wrap3(ch("translation_handle", 3, "translation_offset", "translation_half_range"), t0),
                rotation: match ch("rotation_handle", 4, "rotation_offset", "rotation_half_range") {
                    Some(spline) => Channel::BSpline { start, stop, spline },
                    None => Channel::Static(r0),
                },
                euler: None,
                scale: match ch("scale_handle", 1, "scale_offset", "scale_half_range") {
                    Some(spline) => Channel::BSpline { start, stop, spline },
                    None => Channel::Static(s0),
                },
            })
        }
        _ => None,
    }
}

/// Every NiControllerSequence in a decoded file.
pub fn sequences(file: &crate::NifFile, dec: &Decoded) -> Vec<Sequence> {
    let mut out = Vec::new();
    for (i, b) in file.blocks.iter().enumerate() {
        if b.type_name != "NiControllerSequence" {
            continue;
        }
        let Ok(s) = &dec.blocks[i] else { continue };
        let mut text_keys = Vec::new();
        if let Some(tk) = s.block_ref("text_keys").and_then(|r| dec.get(r)) {
            for k in tk.list("text_keys").unwrap_or_default() {
                if let Value::Struct(k) = k {
                    text_keys.push((k.float("time").unwrap_or(0.0), k.text("value").unwrap_or_default().to_string()));
                }
            }
        }
        let mut tracks = Vec::new();
        for cb in s.list("controlled_blocks").unwrap_or_default() {
            let Value::Struct(cb) = cb else { continue };
            let interp = cb.block_ref("interpolator").unwrap_or(-1);
            let data = match usize::try_from(interp).ok().and_then(|x| file.blocks.get(x)) {
                None => TrackData::None,
                Some(ib) => match transform_track(dec, interp, &ib.type_name) {
                    Some(t) => TrackData::Transform(t),
                    None => TrackData::Other(ib.type_name.clone()),
                },
            };
            let bools = match &data {
                // NiBoolTimelineInterpolator is a NiBoolInterpolator with no fields of its
                // own [documented: nif.xml, schemas/nif/anim.json]: six tracks of the base
                // meshes' sequences use it.
                TrackData::Other(kind) if kind == "NiBoolInterpolator" || kind == "NiBoolTimelineInterpolator" => bool_track(dec, interp),
                _ => None,
            };
            let values = match &data {
                TrackData::Other(kind) => value_track(dec, interp, kind),
                _ => None,
            };
            tracks.push(Track {
                node_name: cb_string(dec, cb, "node_name"),
                controller_type: cb_string(dec, cb, "controller_type"),
                priority: cb.int("priority"),
                data,
                bools,
                controller_id: cb_string(dec, cb, "controller_id"),
                values,
                interpolator_id: cb_string(dec, cb, "interpolator_id"),
            });
        }
        out.push(Sequence {
            block: i,
            name: s.text("name").map(str::to_string),
            start_time: s.float("start_time").unwrap_or(0.0),
            stop_time: s.float("stop_time").unwrap_or(0.0),
            frequency: s.float("frequency").unwrap_or(1.0),
            cycle_type: s.int("cycle_type").unwrap_or(0),
            accum_root_name: s.text("accum_root_name").map(str::to_string),
            text_keys,
            tracks,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_places_follow_the_share_of_the_curve() {
        // Two straight spans: 100 units by time 0.5, then 300 more by time 1.
        let path = Keys { kind: KeyKind::Linear, keys: vec![key(0.0, [0.0, 0.0, 0.0]), key(0.5, [100.0, 0.0, 0.0]), key(1.0, [400.0, 0.0, 0.0])] };
        // Along the whole curve over 12 s, as fxdustwhirlwind01.nif's percent keys.
        let percent = Keys { kind: KeyKind::Linear, keys: vec![key(0.0, [0.0]), key(12.0, [1.0])] };
        let by_time = PathChannel::new(path.clone(), percent.clone(), false);
        let by_length = PathChannel::new(path, percent, true);
        // Halfway in time: the middle key by key time, half of the 400 units by length.
        assert!((by_time.sample(6.0).unwrap()[0] - 100.0).abs() < 1e-3);
        assert!((by_length.sample(6.0).unwrap()[0] - 200.0).abs() < 1.0, "{:?}", by_length.sample(6.0));
        // Held at the ends.
        assert_eq!(by_length.sample(0.0).unwrap()[0], 0.0);
        assert!((by_length.sample(20.0).unwrap()[0] - 400.0).abs() < 1e-3);
        let channel: Channel<3> = Channel::Path(Box::new(by_length));
        assert!(channel.is_animated() && (channel.sample(3.0).unwrap()[0] - 100.0).abs() < 1.0);
    }

    fn morph(name: &str, vectors: Vec<[f32; 3]>) -> MorphTarget {
        MorphTarget { name: Some(name.into()), vectors, weight: None, stored_weight: 0.0, interpolator_type: None }
    }

    #[test]
    fn morphs_add_weighted_offsets_to_the_base_and_take_an_idle_sequence() {
        // A wire of two vertices whose second sags by morph `M01` (relative targets).
        let mut g = GeomMorpher {
            controller: 7,
            target: 26,
            flags: 108,
            start_time: 0.0,
            stop_time: 10.0,
            frequency: 1.0,
            phase: 0.0,
            update_normals: false,
            relative: true,
            num_vertices: 2,
            morphs: vec![morph("Base", vec![[0.0; 3], [100.0, 0.0, 0.0]]), morph("M01", vec![[0.0; 3], [0.0, 0.0, -8.0]])],
        };
        assert!(!g.is_free() && !g.moves());
        let mut out = Vec::new();
        g.positions_at(5.0, &mut out);
        assert_eq!(out, vec![[0.0; 3], [100.0, 0.0, 0.0]]);
        let ramp = Keys { kind: KeyKind::Linear, keys: vec![key(0.0, [0.0]), key(4.0, [1.0])] };
        let cb = |id: &str, values: Option<ValueTrack>| Track {
            node_name: Some("Wire:0".into()),
            controller_type: Some("NiGeomMorpherController".into()),
            priority: Some(0),
            data: TrackData::Other("NiFloatInterpolator".into()),
            bools: None,
            controller_id: None,
            values,
            interpolator_id: Some(id.into()),
        };
        let seq = Sequence {
            block: 5,
            name: Some("Idle".into()),
            start_time: 0.0,
            stop_time: 4.0,
            frequency: 1.0,
            cycle_type: 0,
            accum_root_name: None,
            text_keys: Vec::new(),
            tracks: vec![cb("Base", Some(ValueTrack::Float(Channel::Static(Some([1.0]))))), cb("M01", Some(ValueTrack::Float(Channel::Keys(ramp))))],
        };
        assert_eq!(g.take_sequence(&seq, "Other:0"), 0);
        assert_eq!(g.take_sequence(&seq, "Wire:0"), 2);
        // The sequence's loop replaces the controller's clamp.
        assert_eq!(((g.flags >> 1) & 3, g.stop_time), (0, 4.0));
        assert!(g.moves());
        g.positions_at(2.0, &mut out);
        assert_eq!(out, vec![[0.0; 3], [100.0, 0.0, -4.0]]);
    }

    fn track(rotation: Channel<4>) -> TransformTrack {
        TransformTrack { translation: Channel::Static(None), rotation, euler: None, scale: Channel::Static(None) }
    }

    #[test]
    fn broken_keys_sample_to_finite_values_or_none() {
        let times = [f32::NAN, f32::NEG_INFINITY, -1.0, 0.0, 0.25, 0.5, 1.0, 3.0, f32::INFINITY, f32::MAX];
        // No keys, keys out of order, three keys at one time, NaN and infinite values and
        // times, values whose interpolation overflows; under every kind of interpolation,
        // with tangents and TBC parameters that overflow too.
        let sets: Vec<Vec<Key<3>>> = vec![
            Vec::new(),
            vec![key(1.0, [1.0; 3]), key(0.0, [0.0; 3]), key(0.5, [2.0; 3])],
            vec![key(0.5, [1.0; 3]), key(0.5, [2.0; 3]), key(0.5, [3.0; 3])],
            vec![key(0.0, [f32::NAN; 3]), key(1.0, [0.0; 3])],
            vec![key(f32::NAN, [0.0; 3]), key(1.0, [f32::INFINITY; 3])],
            vec![key(0.0, [-3.0e38; 3]), key(1.0, [3.0e38; 3])],
        ];
        for kind in [KeyKind::Linear, KeyKind::Quadratic, KeyKind::Tbc, KeyKind::XyzRotation, KeyKind::Constant] {
            for ks in &sets {
                for wild in [false, true] {
                    let mut ks = ks.clone();
                    if wild {
                        for k in &mut ks {
                            k.forward = [1.0e38; 3];
                            k.backward = [-1.0e38; 3];
                            k.tbc = [5.0e18, -5.0e18, 1.0e19];
                        }
                    }
                    let c = Channel::Keys(Keys { kind, keys: ks });
                    for t in times {
                        assert!(c.sample(t).is_none_or(|v| finite(&v)), "{kind:?} at {t}");
                    }
                }
            }
        }
        assert_eq!(Channel::<3>::Keys(Keys { kind: KeyKind::Linear, keys: Vec::new() }).sample(0.0), None);
        assert_eq!(Channel::Static(Some([f32::NAN])).sample(0.0), None);
        // Good keys still sample.
        let good = Channel::Keys(Keys { kind: KeyKind::Linear, keys: vec![key(0.0, [0.0]), key(1.0, [2.0])] });
        assert_eq!(good.sample(0.5), Some([1.0]));
    }

    #[test]
    fn broken_rotations_are_unit_quaternions_or_none() {
        let quats = [[0.0; 4], [f32::NAN, 0.0, 0.0, 0.0], [f32::INFINITY, 0.0, 0.0, 0.0], [1.0e30, 1.0e30, 0.0, 0.0], [-3.0e38, 3.0e38, 3.0e38, 0.0], [1.0e-30, 0.0, 0.0, 0.0]];
        for kind in [KeyKind::Linear, KeyKind::Tbc, KeyKind::Constant] {
            for a in quats {
                for b in quats {
                    let t = track(Channel::Keys(Keys { kind, keys: vec![key(0.0, a), key(1.0, b)] }));
                    for time in [f32::NAN, 0.0, 0.3, 0.5, 1.0, f32::INFINITY] {
                        if let Some(q) = t.rotation_at(time) {
                            let len = q.iter().map(|x| x * x).sum::<f32>().sqrt();
                            assert!((len - 1.0).abs() < 1e-3, "{a:?} to {b:?} at {time}: {q:?}");
                        }
                    }
                }
            }
        }
        // Huge but finite values keep their direction (their squares overflowed to a
        // zero quaternion before).
        let q = track(Channel::Static(Some([1.0e30, 1.0e30, 0.0, 0.0]))).rotation_at(0.0).unwrap();
        assert!((q[0] - q[1]).abs() < 1e-6 && (q[0] - 0.5f32.sqrt()).abs() < 1e-4);
        // NaN Euler angles give no rotation.
        let e = |v: f32| Keys { kind: KeyKind::Linear, keys: vec![key(0.0, [v])] };
        let t = TransformTrack { euler: Some([e(0.0), e(f32::NAN), e(0.0)]), ..track(Channel::Static(None)) };
        assert_eq!(t.rotation_at(0.0), None);
    }

    #[test]
    fn broken_splines_and_paths_do_not_panic() {
        // No points, points not a multiple of the width, a width of zero, NaN points,
        // points whose blend overflows.
        let splines = [(3, vec![]), (3, vec![1.0; 7]), (0, vec![1.0; 4]), (3, vec![f32::NAN; 12]), (1, vec![3.0e38, -3.0e38, 3.0e38, -3.0e38, 3.0e38])];
        for (dim, points) in splines {
            let spline = BSplineChannel { dim, points };
            for (start, stop) in [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (f32::NAN, 1.0), (0.0, f32::INFINITY)] {
                let c3 = Channel::<3>::BSpline { start, stop, spline: spline.clone() };
                let c1 = Channel::<1>::BSpline { start, stop, spline: spline.clone() };
                for t in [f32::NAN, -1.0, 0.0, 0.5, 1.0, 2.0, f32::INFINITY] {
                    assert!(c3.sample(t).is_none_or(|v| finite(&v)));
                    assert!(c1.sample(t).is_none_or(|v| finite(&v)));
                    let _ = spline.eval(t);
                }
            }
        }
        // A path of one key, a path standing at one place, a NaN percent.
        let one = Keys { kind: KeyKind::Linear, keys: vec![key(0.0, [1.0, 2.0, 3.0])] };
        let still = Keys { kind: KeyKind::Linear, keys: vec![key(0.0, [1.0; 3]), key(1.0, [1.0; 3])] };
        let nan = Keys { kind: KeyKind::Linear, keys: vec![key(0.0, [f32::NAN]), key(1.0, [f32::NAN])] };
        for path in [one, still] {
            for constant_velocity in [false, true] {
                let c = Channel::<3>::Path(Box::new(PathChannel::new(path.clone(), nan.clone(), constant_velocity)));
                for t in [f32::NAN, 0.0, 0.5, f32::INFINITY] {
                    assert!(c.sample(t).is_some_and(|v| finite(&v)));
                }
            }
        }
    }

    fn key<const N: usize>(time: f32, value: [f32; N]) -> Key<N> {
        Key { time, value, forward: [0.0; N], backward: [0.0; N], tbc: [0.0; 3] }
    }

    #[test]
    fn bool_tracks_step() {
        let keyless = BoolTrack { value: Some(false), keys: Vec::new() };
        assert_eq!(keyless.at(0.5), Some(false));
        assert_eq!(BoolTrack::default().at(0.0), None);
        // 2hhequip.kf: hidden until the weapon is in the hand.
        let equip = BoolTrack { value: None, keys: vec![(0.0, false), (0.5, false), (0.533, true), (1.0, true)] };
        assert_eq!(equip.at(-1.0), Some(false));
        assert_eq!(equip.at(0.52), Some(false));
        assert_eq!(equip.at(0.533), Some(true));
        assert_eq!(equip.at(9.0), Some(true));
    }

    #[test]
    fn linear_and_constant_keys() {
        let mut k = Keys { kind: KeyKind::Linear, keys: vec![key(0.0, [0.0]), key(2.0, [10.0])] };
        assert_eq!(k.sample(1.0), Some([5.0]));
        assert_eq!(k.sample(-1.0), Some([0.0]));
        assert_eq!(k.sample(3.0), Some([10.0]));
        k.kind = KeyKind::Constant;
        assert_eq!(k.sample(1.9), Some([0.0]));
    }

    #[test]
    fn hermite_hits_endpoints_and_tbc_zero_is_catmull_rom() {
        let k = Keys {
            kind: KeyKind::Tbc,
            keys: vec![key(0.0, [0.0]), key(1.0, [1.0]), key(2.0, [4.0]), key(3.0, [9.0])],
        };
        assert_eq!(k.sample(1.0), Some([1.0]));
        assert_eq!(k.sample(2.0), Some([4.0]));
        // Catmull-Rom between 1 and 4 with tangents 2 and 4 at u=0.5:
        // 0.5·1 + 0.125·2 + 0.5·4 − 0.125·4 = 2.25.
        let v = k.sample(1.5).unwrap()[0];
        assert!((v - 2.25).abs() < 1e-5, "{v}");
    }

    #[test]
    fn bspline_array_matches_vec() {
        let pts: Vec<f32> = (0..21).map(|i| ((i * 7) % 5) as f32 - 0.5 * i as f32).collect();
        let c = BSplineChannel { dim: 3, points: pts };
        for i in 0..=16 {
            let u = i as f32 / 16.0;
            assert_eq!(c.eval_array::<3>(u).unwrap().to_vec(), c.eval(u));
        }
        assert_eq!(c.eval_array::<4>(0.5), None);
        let one = BSplineChannel { dim: 4, points: vec![1.0, 2.0, 3.0, 4.0] };
        assert_eq!(one.eval_array::<4>(0.7), Some([1.0, 2.0, 3.0, 4.0]));
    }

    #[test]
    fn quadratic_tangents_leave_backward_arrive_forward() {
        // The spinning windmill's blades: two turns in 7 s, the tangent leaving the first
        // key in `backward` and the one arriving at the last in `forward`; a steady spin.
        let turn = 4.0 * std::f32::consts::PI;
        let mut a = key(0.0, [0.0]);
        a.backward = [turn];
        let mut b = key(7.0, [turn]);
        b.forward = [turn];
        let k = Keys { kind: KeyKind::Quadratic, keys: vec![a, b] };
        for t in [1.0f32, 1.75, 3.5, 6.0] {
            let v = k.sample(t).unwrap()[0];
            assert!((v - turn * t / 7.0).abs() < 1e-4, "{t}: {v}");
        }
    }

    #[test]
    fn bspline_endpoints_and_constant() {
        let flat = BSplineChannel { dim: 1, points: vec![2.0; 6] };
        for u in [0.0, 0.3, 1.0] {
            assert!((flat.eval(u)[0] - 2.0).abs() < 1e-6);
        }
        // Open uniform splines interpolate their end control points.
        let ramp = BSplineChannel { dim: 2, points: vec![0.0, 5.0, 1.0, 5.0, 2.0, 5.0, 3.0, 5.0, 4.0, 5.0] };
        assert_eq!(ramp.eval(0.0), [0.0, 5.0]);
        let end = ramp.eval(1.0);
        assert!((end[0] - 4.0).abs() < 1e-6 && (end[1] - 5.0).abs() < 1e-6, "{end:?}");
        // A linear control polygon gives a monotone curve.
        let mut last = -1.0;
        for i in 0..=20 {
            let v = ramp.eval(i as f32 / 20.0)[0];
            assert!(v >= last);
            last = v;
        }
    }

    #[test]
    fn slerp_takes_short_arc() {
        let a = [1.0, 0.0, 0.0, 0.0];
        let b = [-1.0, 0.0, 0.0, 0.0]; // the same rotation, opposite sign
        let m = slerp(a, b, 0.5);
        let n: f32 = m.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((n - 1.0).abs() < 1e-5);
        let half = std::f32::consts::FRAC_1_SQRT_2;
        let q90 = [half, 0.0, 0.0, half]; // 90° about z
        let q45 = slerp(a, q90, 0.5);
        assert!((q45[0] - (std::f32::consts::PI / 8.0).cos()).abs() < 1e-5, "{q45:?}");
    }

    #[test]
    fn euler_identity() {
        let q = euler_xyz_to_quat([0.0, 0.0, 0.0]);
        assert_eq!(q, [1.0, 0.0, 0.0, 0.0]);
        let q = euler_xyz_to_quat([0.0, 0.0, std::f32::consts::PI]);
        assert!((q[3] - 1.0).abs() < 1e-6);
    }
}

/// One morph target of a [`GeomMorpher`]: a vector per vertex of the shape
/// (docs/spec-notes/vertex-morphs.md §1).
#[derive(Debug, Clone, PartialEq)]
pub struct MorphTarget {
    /// The morph's frame name (`Morph.frame_name` [documented: nif.xml]).
    pub name: Option<String>,
    /// One vector per vertex (`Morph.vectors`).
    pub vectors: Vec<[f32; 3]>,
    /// Its weight over time: the interpolator of the controller's matching
    /// `MorphWeight` entry, evaluated (`None`: no interpolator or not a float one).
    pub weight: Option<ValueTrack>,
    /// The `MorphWeight.weight` stored beside that interpolator [documented: nif.xml].
    pub stored_weight: f32,
    /// The interpolator's type name, for surveys (`None`: null reference).
    pub interpolator_type: Option<String>,
}

/// A `NiGeomMorpherController` with its `NiMorphData` (docs/spec-notes/vertex-morphs.md):
/// what it drives, how it runs and its morph targets, morph `i` weighted by the
/// controller's `i`-th `MorphWeight` [documented: nif.xml NiGeomMorpherController,
/// NiMorphData, Morph, MorphWeight].
#[derive(Debug, Clone, PartialEq)]
pub struct GeomMorpher {
    /// Block index of the controller.
    pub controller: usize,
    /// Block index of the shape it drives (`target`), -1 for none.
    pub target: i32,
    /// NiTimeController flags (bit 3 Active, bit 5 manager controlled, bits 1-2 cycle
    /// type [documented: nif.xml TimeControllerFlags]).
    pub flags: i64,
    pub start_time: f32,
    pub stop_time: f32,
    pub frequency: f32,
    pub phase: f32,
    /// `morpher_flags` is UPDATE_NORMALS_ENABLED (1) [documented: nif.xml GeomMorpherFlags].
    pub update_normals: bool,
    /// `NiMorphData.relative_targets` is not 0 ("always 1 in all official files"
    /// [documented: nif.xml]).
    pub relative: bool,
    /// The vertex count `NiMorphData` states.
    pub num_vertices: usize,
    pub morphs: Vec<MorphTarget>,
}

impl GeomMorpher {
    /// Whether no sequence drives it and it runs by itself: Active and not manager
    /// controlled [documented: nif.xml TimeControllerFlags].
    pub fn is_free(&self) -> bool {
        self.flags & 0x8 != 0 && self.flags & 0x20 == 0
    }

    /// The weight of morph `i` at controller time `t`: its interpolator's value, else
    /// the stored weight.
    pub fn weight_at(&self, i: usize, t: f32) -> f32 {
        let m = &self.morphs[i];
        m.weight.as_ref().and_then(|w| w.at(t)).map_or(m.stored_weight, |v| v[0])
    }

    /// The vertex positions at controller time `t` into `out` (resized to
    /// `num_vertices`): with relative targets, morph 0 plus every later morph times its
    /// weight; without, the weighted sum of all morphs [hypothesis: nif.xml names the
    /// fields only; morph 0 equals the shape's stored vertices in every base-game file
    /// with free morphs and the later morphs are offsets of a few units,
    /// docs/spec-notes/vertex-morphs.md §2].
    pub fn positions_at(&self, t: f32, out: &mut Vec<[f32; 3]>) {
        out.clear();
        out.resize(self.num_vertices, [0.0; 3]);
        for (i, m) in self.morphs.iter().enumerate() {
            let w = if self.relative && i == 0 { 1.0 } else { self.weight_at(i, t) };
            if w == 0.0 {
                continue;
            }
            for (p, v) in out.iter_mut().zip(&m.vectors) {
                p[0] += w * v[0];
                p[1] += w * v[1];
                p[2] += w * v[2];
            }
        }
    }

    /// Feed the morphs from the tracks of `seq` that drive a `NiGeomMorpherController`
    /// on the shape named `shape`, matched by interpolator ID to the morph's frame name
    /// (docs/spec-notes/vertex-morphs.md §1), and take the sequence's timing (start,
    /// stop, frequency, cycle type) as the controller's. The number of morphs fed.
    pub fn take_sequence(&mut self, seq: &Sequence, shape: &str) -> usize {
        let mut fed = 0;
        for t in &seq.tracks {
            if t.controller_type.as_deref() != Some("NiGeomMorpherController") || t.node_name.as_deref() != Some(shape) {
                continue;
            }
            let (Some(id), Some(values)) = (&t.interpolator_id, &t.values) else { continue };
            if let Some(m) = self.morphs.iter_mut().find(|m| m.name.as_deref() == Some(id.as_str())) {
                m.weight = Some(values.clone());
                fed += 1;
            }
        }
        if fed > 0 {
            self.start_time = seq.start_time;
            self.stop_time = seq.stop_time;
            self.frequency = seq.frequency;
            self.phase = 0.0;
            self.flags = (self.flags & !0x6) | ((seq.cycle_type & 3) << 1);
        }
        fed
    }

    /// Whether any morph after the base has a weight that changes, or a constant
    /// non-zero weight on a non-zero morph: the shape moves or differs from morph 0.
    pub fn moves(&self) -> bool {
        let start = if self.relative { 1 } else { 0 };
        self.morphs.iter().enumerate().skip(start).any(|(i, m)| {
            if m.vectors.iter().all(|v| v.iter().all(|x| *x == 0.0)) {
                return false;
            }
            let span = (self.stop_time - self.start_time).max(0.0);
            let first = self.weight_at(i, self.start_time);
            (1..=32).any(|k| (self.weight_at(i, self.start_time + span * k as f32 / 32.0) - first).abs() > 1e-4)
        })
    }
}

/// Every `NiGeomMorpherController` of a file with a readable `NiMorphData`.
pub fn geom_morphers(file: &crate::NifFile, dec: &Decoded) -> Vec<GeomMorpher> {
    let mut out = Vec::new();
    for (i, b) in file.blocks.iter().enumerate() {
        if b.type_name != "NiGeomMorpherController" {
            continue;
        }
        let Some(c) = dec.get(i as i32) else { continue };
        let Some(data) = c.block_ref("data").and_then(|r| dec.get(r)) else { continue };
        // The stored count sizes `positions_at`'s output; a broken file's count (with no
        // morphs whose vectors bound it) is cut to the longest morph's.
        let stored_vertices = usize::try_from(data.int("num_vertices").unwrap_or(0)).unwrap_or(0);
        let weights: Vec<(i32, f32)> = c
            .list("interpolator_weights")
            .unwrap_or_default()
            .iter()
            .filter_map(|w| match w {
                Value::Struct(w) => Some((w.block_ref("interpolator").unwrap_or(-1), w.float("weight").unwrap_or(0.0))),
                _ => None,
            })
            .collect();
        let morphs = data
            .list("morphs")
            .unwrap_or_default()
            .iter()
            .enumerate()
            .filter_map(|(k, m)| {
                let Value::Struct(m) = m else { return None };
                let vectors = m.floats("vectors").map(|f| f.as_chunks::<3>().0.to_vec()).unwrap_or_default();
                let (interp, stored_weight) = weights.get(k).copied().unwrap_or((-1, 0.0));
                let interpolator_type = usize::try_from(interp).ok().and_then(|x| file.blocks.get(x)).map(|b| b.type_name.clone());
                let weight = interpolator_type.as_deref().and_then(|ty| value_track(dec, interp, ty));
                Some(MorphTarget { name: m.text("frame_name").map(str::to_string), vectors, weight, stored_weight, interpolator_type })
            })
            .collect::<Vec<MorphTarget>>();
        let num_vertices = stored_vertices.min(morphs.iter().map(|m| m.vectors.len()).max().unwrap_or(0));
        out.push(GeomMorpher {
            controller: i,
            target: c.block_ref("target").unwrap_or(-1),
            flags: c.int("flags").unwrap_or(0),
            start_time: c.float("start_time").unwrap_or(0.0),
            stop_time: c.float("stop_time").unwrap_or(0.0),
            frequency: c.float("frequency").unwrap_or(1.0),
            phase: c.float("phase").unwrap_or(0.0),
            update_normals: c.int("morpher_flags") == Some(1),
            relative: data.int("relative_targets").unwrap_or(1) != 0,
            num_vertices,
            morphs,
        });
    }
    out
}
