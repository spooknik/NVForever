//! Following a path: [`Follower`] says where to head from the actor's current feet
//! (whatever moves the actor: the physics character controller, or [`Walker`], which
//! keeps the actor on the navmesh surface without physics).
//!
//! Arrival radii, turn rates and the stuck rule are engine choices
//! (docs/design/navmesh.md §4), not observed game values.

use crate::graph::{DoorEdge, NavGraph, Space, NONE};
use crate::jump::Flight;
use crate::path::{Leg, LedgeHop, Path, PathError, PathParams, Place};
use serde::Serialize;

/// What to do next.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Steer {
    /// Head for `target`; `dir` is the horizontal unit direction, `to_target` the
    /// horizontal distance to it, `remaining` the horizontal length left on the path
    /// (to its end, or to the door that ends this leg).
    Walk { target: [f32; 3], dir: [f32; 2], to_target: f32, remaining: f32 },
    /// At a door: move the actor to `arrival` (in the next leg's space), then call
    /// [`Follower::door_used`]. An actor that can turn first stops and faces the door
    /// ([`Walker::face_door`]) while this is given.
    Door(DoorEdge),
    /// At the stop in front of a door that opens in place (`at`): stand, open the door,
    /// and once it is fully open (its "Open" sequence has ended) call
    /// [`Follower::door_opened`]. Until then the follower holds here. A door already
    /// open is passed at once by calling `door_opened` straight away.
    OpenDoor { door: u32, at: [f32; 3] },
    /// At the end of the path.
    Arrived,
    /// The feet are within horizontal reach of what comes next on the path (a corner, the
    /// end, a door) but not at its height ([`HEIGHT_TOLERANCE`]): they are on another
    /// floor, or have fallen, and the path no longer fits them. Nothing was consumed. Not
    /// an arrival: the caller stops and plans again from the feet ([`Walker::replan`]).
    OffPath,
}

/// Where along a path an actor is.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Follower {
    pub path: Path,
    pub leg: usize,
    /// The point of the leg being walked to.
    pub next: usize,
    /// A corner counts as reached within this horizontal distance.
    pub arrive_radius: f32,
    /// The end of the path counts as reached within this horizontal distance.
    pub goal_radius: f32,
    /// A door that ends a leg is used from within this horizontal distance of the leg's
    /// end (in front of the door, where the frame may keep a body from the exact point).
    pub door_radius: f32,
    /// The next stop of the leg's `door_stops` (doors that open in place).
    pub stop: usize,
    /// Held at that stop ([`Steer::OpenDoor`] given) until [`Follower::door_opened`]:
    /// a door pushing the actor about while it opens does not send it walking again.
    pub holding: bool,
    /// How far the feet stood from the path's surface when it was planned again, when
    /// that was past [`HEIGHT_TOLERANCE`] (above: positive): on something the navmesh does
    /// not know, or a navmesh that lies above its collision. The tolerance reaches that
    /// much further on that side, and the surface itself still fits, so such feet can walk
    /// on where the navmesh shows no other floor ([`Follower::replanned`]). 0 otherwise.
    pub height_offset: f32,
}

/// A ledge's take-off counts as reached within this horizontal distance: the actor goes
/// to the edge before it jumps [design].
pub const TAKE_OFF_RADIUS: f32 = 2.0;

/// How far the feet may be from the path's height where they stand and still be on its
/// surface, game units [design: an engineering number, not measured safe]. Its reasoning:
/// the character's step height (24, `fnv_phys::CharacterParams`) and the terrain's collision
/// shell (3.5) leave about 12 units for the capsule's shape and for the navmesh not
/// matching the collision, and it equals [`crate::path::LEVEL_GAP`], the height at which
/// path planning already calls a surface another level. Nothing bounds how far an authored
/// navmesh is from its collision, and floors can be stacked closer than this
/// (docs/design/navmesh.md §4).
pub const HEIGHT_TOLERANCE: f32 = 40.0;

/// A new path may start at most this far above the feet ([`Follower::replanned`]), game
/// units [design: as far as `NavGraph::nearest` takes a surface for the one under a point;
/// less than a person's height, so what lies this near over the feet is not a floor they
/// stood under but a navmesh above its collision]. Further up, it is taken as another
/// floor they cannot get back onto from there.
pub const REPLAN_BELOW_LIMIT: f32 = 64.0;

fn hdist(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// The height of segment `a`-`b` where it passes closest to `feet` seen from above (the
/// projection clamped to the segment). `None` for a segment with no horizontal length,
/// which says nothing about a slope.
fn segment_height(a: [f32; 3], b: [f32; 3], feet: [f32; 3]) -> Option<f32> {
    let (sx, sy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = sx * sx + sy * sy;
    if len2 < 1e-6 {
        return None;
    }
    let t = (((feet[0] - a[0]) * sx + (feet[1] - a[1]) * sy) / len2).clamp(0.0, 1.0);
    Some(a[2] + (b[2] - a[2]) * t)
}

/// Whether feet at height `z` count as on a surface at height `surface`, for a path with
/// [`Follower::height_offset`] `offset`.
fn height_fits(z: f32, surface: f32, offset: f32) -> bool {
    let above = z - surface;
    above >= offset.min(0.0) - HEIGHT_TOLERANCE && above <= offset.max(0.0) + HEIGHT_TOLERANCE
}

/// Whether `feet` count as on the surface around point `i` of `leg`: the walking segment
/// that ends there, at the feet's place seen from above, so a slope walked is not a height
/// error (a fixed difference from the point would reject the last steps up any steep
/// slope, and adding radius times slope could admit another floor); the point itself when
/// that segment is a ledge link's flight (never interpolated) or has no horizontal length.
/// An ordinary corner also fits by its outgoing segment: an actor walking down the slope
/// beyond a corner is far below the corner and still where the path goes.
fn point_fits(leg: &Leg, i: usize, feet: [f32; 3], offset: f32) -> bool {
    let q = leg.points[i];
    let flown_to = leg.hops.iter().any(|h| h.point + 1 == i);
    let incoming = i.checked_sub(1).filter(|_| !flown_to).and_then(|p| segment_height(leg.points[p], q, feet));
    if height_fits(feet[2], incoming.unwrap_or(q[2]), offset) {
        return true;
    }
    let ordinary = !leg.hops.iter().any(|h| h.point == i) && !leg.door_stops.iter().any(|s| s.point == i);
    ordinary && leg.points.get(i + 1).and_then(|&n| segment_height(q, n, feet)).is_some_and(|h| height_fits(feet[2], h, offset))
}

impl Follower {
    pub fn new(path: Path) -> Follower {
        Follower { path, leg: 0, next: 1, arrive_radius: 16.0, goal_radius: 1.0, door_radius: 48.0, stop: 0, holding: false, height_offset: 0.0 }
    }

    /// The space of the leg being walked.
    pub fn space(&self) -> Option<Space> {
        self.path.legs.get(self.leg).map(|l| l.space)
    }

    /// Advance past reached corners and say where to head from `feet`. Distances and
    /// directions are horizontal; a corner, a door or the end only counts as reached with
    /// the feet at its height, else [`Steer::OffPath`].
    pub fn steer(&mut self, feet: [f32; 3]) -> Steer {
        self.steer_judged(feet, true)
    }

    /// [`Follower::steer`] with the height left out, for feet that are not on the ground
    /// by the path's rules: a swimmer's are at the water's surface, the navmesh on the bed.
    pub fn steer_any_height(&mut self, feet: [f32; 3]) -> Steer {
        self.steer_judged(feet, false)
    }

    /// Whether `feet` count as on the surface of the part of the path being walked. Not
    /// judged while crossing a ledge link: an actor in the air is not on any surface yet.
    pub fn fits(&self, feet: [f32; 3]) -> bool {
        let Some(leg) = self.path.legs.get(self.leg) else { return true };
        let Some(last) = leg.points.len().checked_sub(1) else { return true };
        let i = self.next.min(last);
        (self.next <= last && leg.hops.iter().any(|h| h.point + 1 == i)) || point_fits(leg, i, feet, self.height_offset)
    }

    fn steer_judged(&mut self, feet: [f32; 3], judged: bool) -> Steer {
        let Some(leg) = self.path.legs.get(self.leg) else { return Steer::Arrived };
        let pts = &leg.points;
        let pending = leg.door_stops.get(self.stop).copied();
        // A door stop reached, or at or behind the corner being walked to, holds the
        // follower there.
        if let Some(s) = pending.filter(|s| self.holding || s.point < self.next) {
            // Once held it stays held, wherever the door pushes the actor; holding starts
            // only from the door's own floor.
            if judged && !self.holding && !point_fits(leg, s.point, feet, self.height_offset) {
                return Steer::OffPath;
            }
            self.holding = true;
            return Steer::OpenDoor { door: s.door, at: pts[s.point] };
        }
        while self.next < pts.len() {
            let q = pts[self.next];
            let d = hdist(feet, q);
            let last = self.next + 1 == pts.len();
            let stop = pending.filter(|s| s.point == self.next);
            let goal = last && leg.door.is_none() && self.leg + 1 == self.path.legs.len();
            let radius = if stop.is_some() {
                self.arrive_radius
            } else if leg.hops.iter().any(|h| h.point == self.next) {
                TAKE_OFF_RADIUS
            } else if goal {
                self.goal_radius
            } else if last && leg.door.is_some() {
                self.door_radius
            } else {
                self.arrive_radius
            };
            // Passed a corner (beyond it along the segment that ends there) while close.
            let passed = !last && d < 4.0 * self.arrive_radius && {
                let p = pts[self.next - 1];
                let (sx, sy) = (q[0] - p[0], q[1] - p[1]);
                (feet[0] - q[0]) * sx + (feet[1] - q[1]) * sy >= 0.0
            };
            if d <= radius || passed {
                if judged && !point_fits(leg, self.next, feet, self.height_offset) {
                    // The flight over a ledge link is still to come: the landing is reached
                    // from the air, whatever its distance seen from above.
                    let above = feet[2] > q[2];
                    let flight_to_come = leg.hops.iter().any(|h| h.point + 1 == self.next && h.up != above);
                    // Beyond a corner it does not fit: not passed after all, it heads back
                    // to it and is judged there.
                    if d <= radius && !flight_to_come {
                        return Steer::OffPath;
                    }
                    break;
                }
                if let Some(s) = stop {
                    self.holding = true;
                    return Steer::OpenDoor { door: s.door, at: q };
                }
                self.next += 1;
                continue;
            }
            break;
        }
        if self.next >= pts.len() {
            // Not only when the end is first reached: a caller may stand here for frames,
            // turning to a load door, and be moved meanwhile.
            if judged && pts.len().checked_sub(1).is_some_and(|last| !point_fits(leg, last, feet, self.height_offset)) {
                return Steer::OffPath;
            }
            return match leg.door {
                Some(d) => Steer::Door(d),
                None => Steer::Arrived,
            };
        }
        let target = pts[self.next];
        let to_target = hdist(feet, target);
        let dir = if to_target > 1e-4 { [(target[0] - feet[0]) / to_target, (target[1] - feet[1]) / to_target] } else { [0.0, 0.0] };
        let remaining = to_target + pts[self.next..].windows(2).map(|w| hdist(w[0], w[1])).sum::<f32>();
        Steer::Walk { target, dir, to_target, remaining }
    }

    /// The next door on the way, when the actor at `feet` is within `reach` (horizontal
    /// units) of it: the current leg's next door that opens in place (its stop), else the
    /// door that ends the leg. Held at a stop, that stop's door whatever the distance. A
    /// door on another floor than the feet is not ahead, however near seen from above.
    /// For whoever has to ask the door first (`fnv_game::npc_doors`).
    pub fn door_ahead(&self, feet: [f32; 3], reach: f32) -> Option<u32> {
        let leg = self.path.legs.get(self.leg)?;
        if let Some(s) = leg.door_stops.get(self.stop) {
            return (self.holding || (hdist(feet, leg.points[s.point]) <= reach && point_fits(leg, s.point, feet, self.height_offset))).then_some(s.door);
        }
        let d = leg.door?;
        let last = leg.points.len().checked_sub(1)?;
        (hdist(feet, leg.points[last]) <= reach && point_fits(leg, last, feet, self.height_offset)).then_some(d.door)
    }

    /// The door that ended the current leg was used: go on with the next leg.
    pub fn door_used(&mut self) {
        self.leg += 1;
        self.next = 1;
        self.stop = 0;
        self.holding = false;
    }

    /// The door of the current [`Steer::OpenDoor`] is open: walk on through it.
    pub fn door_opened(&mut self) {
        let Some(s) = self.path.legs.get(self.leg).and_then(|l| l.door_stops.get(self.stop)).copied() else { return };
        self.stop += 1;
        self.holding = false;
        self.next = self.next.max(s.point + 1);
    }

    /// Move on along the path `ahead` horizontal units from `feet` (via the corner being
    /// walked to, then the next ones), as far as the leg's next door stop or its end at
    /// most, and return that point (on the path, so on the navmesh; its height is the
    /// path's between two corners). Corners passed on the way count as reached. `None`
    /// while held at a door stop, or with no leg left. For a stuck actor, the failsafe
    /// that puts it past what blocks it ([`Advice::Skip`]).
    pub fn skip_ahead(&mut self, feet: [f32; 3], ahead: f32) -> Option<[f32; 3]> {
        if self.holding {
            return None;
        }
        let leg = self.path.legs.get(self.leg)?;
        let pts = &leg.points;
        let last = pts.len().checked_sub(1)?;
        let limit = leg.door_stops.get(self.stop).map_or(last, |s| s.point.min(last));
        let mut from = feet;
        let mut left = ahead;
        for (i, &q) in pts.iter().enumerate().take(limit + 1).skip(self.next) {
            let seg = hdist(from, q);
            if left < seg {
                let t = left / seg;
                self.next = i;
                return Some([from[0] + (q[0] - from[0]) * t, from[1] + (q[1] - from[1]) * t, from[2] + (q[2] - from[2]) * t]);
            }
            left -= seg;
            from = q;
        }
        // At the door stop or the leg's end: steering from there holds, uses the door or
        // arrives.
        self.next = self.next.max(limit);
        Some(pts[limit])
    }

    /// Whether the rest of the path (the leg being walked and those after it) crosses a
    /// triangle paths may no longer cross (a navmesh disabled since it was planned,
    /// [`NavGraph::usable`]).
    pub fn blocked(&self, nav: &NavGraph) -> bool {
        self.path.legs.iter().skip(self.leg).flat_map(|l| &l.corridor).any(|&t| (t as usize) < nav.tris.len() && !nav.usable(t))
    }

    /// Whether the rest of the path goes through one of `doors`: a load door ending a
    /// leg still ahead, or a door that opens in place on the way.
    pub fn through_doors(&self, doors: &[u32]) -> bool {
        self.path.legs.iter().skip(self.leg).any(|l| l.door.is_some_and(|d| doors.contains(&d.door)) || l.door_stops.iter().any(|s| doors.contains(&s.door)))
    }

    /// The ledge link being crossed: the corner walked to is the landing of one of the
    /// leg's [`LedgeHop`]s (its take-off reached). With the landing point.
    pub fn hop(&self) -> Option<(LedgeHop, [f32; 3])> {
        let leg = self.path.legs.get(self.leg)?;
        let h = *leg.hops.iter().find(|h| h.point + 1 == self.next)?;
        Some((h, *leg.points.get(self.next)?))
    }

    /// The take-off point of the ledge link [`Follower::hop`] names: the path point
    /// before its landing (`LedgeHop::point`).
    pub fn take_off(&self) -> Option<[f32; 3]> {
        let leg = self.path.legs.get(self.leg)?;
        let h = leg.hops.iter().find(|h| h.point + 1 == self.next)?;
        leg.points.get(h.point).copied()
    }

    /// The path's destination.
    pub fn destination(&self) -> Option<Place> {
        let l = self.path.legs.last()?;
        Some(Place { space: l.space, pos: *l.points.last()? })
    }

    /// A new follower for this path's destination, from feet at `from` this path no longer
    /// fits ([`Steer::OffPath`]), with the same radii. Its first segment starts where the
    /// navmesh is under the feet, so it fits them. A path that could only start well above
    /// the feet ([`REPLAN_BELOW_LIMIT`]: they are below every surface near, nothing leads
    /// from them up to it) is no way from here: `Unreachable`. Feet further than the
    /// tolerance from the only surface near show no other floor: the new path allows for
    /// that ([`Follower::height_offset`]).
    pub fn replanned(&self, nav: &NavGraph, from: Place, params: &PathParams) -> Result<Follower, PathError> {
        let to = self.destination().ok_or(PathError::Unreachable { expanded: 0 })?;
        let path = nav.find_path(from, to, params)?;
        let start = path.legs.first().and_then(|l| l.points.first()).map_or(from.pos[2], |p| p[2]);
        let above = from.pos[2] - start;
        if above < -REPLAN_BELOW_LIMIT {
            return Err(PathError::Unreachable { expanded: path.expanded });
        }
        let mut f = Follower::new(path);
        (f.arrive_radius, f.goal_radius, f.door_radius) = (self.arrive_radius, self.goal_radius, self.door_radius);
        f.height_offset = if above.abs() > HEIGHT_TOLERANCE { above } else { 0.0 };
        Ok(f)
    }
}

/// How many times an actor is planned a new way from where it stands before it is taken as
/// having none ([`PathRecovery`]) [design].
pub const RECOVERY_TRIES: u32 = 3;
/// Moved this far (game units, in any direction) since the last try, the tries count from
/// the start again: the way it was given led somewhere [design].
pub const RECOVERY_RESET_DISTANCE: f32 = 256.0;
/// So do they this long after the last try, seconds [design].
pub const RECOVERY_RESET_SECONDS: f64 = 30.0;

/// Bounds plans made again for [`Steer::OffPath`], so an actor no plan helps does not plan
/// on every frame: it fails as a walk with no way there does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct PathRecovery {
    tries: u32,
    at: [f32; 3],
    when: f64,
}

impl PathRecovery {
    /// One more try now, from `feet`, at time `now` (seconds): whether it may be made.
    pub fn allow(&mut self, feet: [f32; 3], now: f64) -> bool {
        let moved = hdist(feet, self.at).hypot(feet[2] - self.at[2]) >= RECOVERY_RESET_DISTANCE;
        if self.tries > 0 && (moved || now - self.when >= RECOVERY_RESET_SECONDS) {
            self.tries = 0;
        }
        if self.tries >= RECOVERY_TRIES {
            return false;
        }
        self.tries += 1;
        (self.at, self.when) = (feet, now);
        true
    }
}

/// What [`Unstuck`] advises.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum Advice {
    /// Head this way (the path's direction, or a detour round an obstacle).
    Head([f32; 2]),
    /// Detours did not help: plan the path again from here.
    Replan,
    /// Detours did not help: move the actor on along its path, past what blocks it
    /// ([`Follower::skip_ahead`]; given instead of [`Advice::Replan`] by an [`Unstuck`]
    /// with `skip` set).
    Skip,
    /// Replanning did not help either.
    GiveUp,
}

/// Getting an actor moved by physics past obstacles the navmesh does not know about
/// (low rocks the navmesh covers but the character controller cannot climb, clutter,
/// other actors). When the actor has got less than a fifth of its speed closer to the end
/// of its path (the path's remaining length) over the last second, it detours: turned 70° one way for 0.6 s, then the other way, then 110° each
/// way; if still stuck, the path is planned again (or, with `skip`, the actor is moved on
/// along it: the failsafe the owner asked for on 2026-10-17); after `max_replans`, it
/// gives up. An engine choice (docs/design/navmesh.md §4), not the original game's behaviour.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unstuck {
    pub speed: f32,
    pub max_replans: usize,
    /// Advise [`Advice::Skip`] where it would advise [`Advice::Replan`].
    pub skip: bool,
    /// The path's remaining length over the last second, one sample per update with its
    /// time step.
    #[serde(skip)]
    history: std::collections::VecDeque<(f32, f32)>,
    /// The detour being made: index into [`DETOURS`] and time left.
    detour: Option<(usize, f32)>,
    /// Detours tried since the actor got stuck.
    tried: usize,
    /// The remaining length where the actor got stuck: detours count as having helped
    /// only once the actor is half a second's walk closer than this.
    stuck_at: Option<f32>,
    /// [`Unstuck::advise_moved`]: where the feet were and which way the actor was
    /// heading at the last update, and the progress so far, negated (counting down from
    /// 0, as a remaining length would).
    #[serde(skip)]
    last: Option<([f32; 2], [f32; 2])>,
    #[serde(skip)]
    moved_left: f32,
    pub detours: usize,
    /// Replans (or skips) advised.
    pub replans: usize,
    /// An [`Advice::Skip`] was given ([`Unstuck::after_skip`] for the stuck rule of the
    /// actor put down where it was moved on to).
    pub skipped: bool,
}

/// Detour turns (radians, positive = to the left / counter-clockwise seen from above).
const DETOURS: [f32; 4] = [1.22, -1.22, 1.92, -1.92];
const DETOUR_SECONDS: f32 = 0.6;

fn rotate(d: [f32; 2], a: f32) -> [f32; 2] {
    let (s, c) = a.sin_cos();
    [d[0] * c - d[1] * s, d[0] * s + d[1] * c]
}

impl Unstuck {
    pub fn new(speed: f32) -> Unstuck {
        Unstuck { speed, max_replans: 5, skip: false, history: Default::default(), detour: None, tried: 0, stuck_at: None, last: None, moved_left: 0.0, detours: 0, replans: 0, skipped: false }
    }

    /// For an actor just moved on along its path ([`Advice::Skip`]): the detours were
    /// tried where it got stuck, so if it is stuck again before it has made a second's
    /// progress it is moved on again at once (a second without progress), not after the
    /// four detours. Once it gets on, the detours come back [design: a stretch blocked for
    /// longer than one skip (Boone just past his room's door, follower-boone smoke,
    /// 2026-10-17) cost 7.4 s per 160 units].
    pub fn after_skip(&mut self) {
        self.tried = DETOURS.len();
    }

    /// Forget the progress history (after a door, a teleport or a new path).
    pub fn reset(&mut self) {
        self.history.clear();
        self.detour = None;
        self.stuck_at = None;
        self.last = None;
    }

    /// As [`Unstuck::advise`], with progress measured by how far the feet moved along the
    /// way the actor was to go (`want` of the last update), not by the path's remaining
    /// length: for paths planned again and again after a moving target (Follow packages,
    /// owner's playtest 2026-10-17: a pack brahmin running after its merchant kept a
    /// path of about the same length, counted as stuck all the way and detoured into a
    /// fence). Sliding along an obstacle, across the way, is still no progress.
    pub fn advise_moved(&mut self, feet: [f32; 3], want: [f32; 2], dt: f32) -> Advice {
        if let Some((at, way)) = self.last {
            self.moved_left -= (feet[0] - at[0]) * way[0] + (feet[1] - at[1]) * way[1];
        }
        self.last = Some(([feet[0], feet[1]], want));
        self.advise(self.moved_left, want, dt)
    }

    /// Record the path's remaining length after a step of `dt` seconds and say which way
    /// to head next, given the path's direction `want`. Moving without getting closer
    /// along the path (sliding round an obstacle, a detour that does not help) is not
    /// progress.
    pub fn advise(&mut self, remaining: f32, want: [f32; 2], dt: f32) -> Advice {
        self.history.push_back((remaining, dt));
        let mut span: f32 = self.history.iter().map(|h| h.1).sum();
        while span - self.history.front().map_or(0.0, |h| h.1) >= 1.0 {
            span -= self.history.pop_front().expect("non-empty").1;
        }
        if let Some((i, left)) = self.detour {
            let left = left - dt;
            if left > 0.0 {
                self.detour = Some((i, left));
                return Advice::Head(rotate(want, DETOURS[i]));
            }
            self.detour = None;
            self.history.clear();
            self.history.push_back((remaining, dt));
            return Advice::Head(want);
        }
        let first = self.history.front().expect("just pushed").0;
        let progress = first - remaining;
        if span < 1.0 {
            return Advice::Head(want);
        }
        if progress >= 0.2 * self.speed {
            if self.stuck_at.is_none_or(|m| remaining < m - 0.5 * self.speed) {
                self.tried = 0;
                self.stuck_at = None;
            }
            return Advice::Head(want);
        }
        // Stuck.
        self.history.clear();
        self.stuck_at = Some(self.stuck_at.map_or(remaining, |m| m.min(remaining)));
        if self.tried < DETOURS.len() {
            let i = self.tried;
            self.tried += 1;
            self.detours += 1;
            self.detour = Some((i, DETOUR_SECONDS));
            return Advice::Head(rotate(want, DETOURS[i]));
        }
        self.tried = 0;
        if self.replans >= self.max_replans {
            return Advice::GiveUp;
        }
        self.replans += 1;
        if self.skip {
            self.skipped = true;
            Advice::Skip
        } else {
            Advice::Replan
        }
    }
}

/// What a [`Walker`] did in an update.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum WalkEvent {
    /// No path, or standing at its end.
    Idle,
    Moved,
    /// Stood and turned on the spot towards the way to walk (not there yet, or just now).
    Turned,
    /// Used a door; the walker is now at its arrival, in another space or place.
    UsedDoor(DoorEdge),
    /// Reached the end of the path in this update.
    Arrived,
    /// Standing in front of a door that opens in place, waiting for it to open.
    WaitingForDoor(u32),
    /// The path no longer fits the feet ([`Steer::OffPath`]): standing where it is, the
    /// path kept, for the caller to plan again ([`Walker::replan`]). Not an arrival.
    PathLost,
}

/// An actor moved along a path on the navmesh surface, without physics (the actor's
/// height is the navmesh's under it). For actors with no collision world around them;
/// the engine moves actors with collision through the character controller instead.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Walker {
    pub space: Space,
    pub feet: [f32; 3],
    /// Compass heading, radians (0 north, π/2 east).
    pub heading: f32,
    /// The triangle under the feet, or [`NONE`].
    pub tri: u32,
    pub speed: f32,
    /// The share of [`Self::speed`] it moves at (1: all of it): an actor's crippled legs
    /// slow it whatever speed its package or fight asks for ([`Self::moving_speed`]).
    pub pace: f32,
    /// Fastest turn while walking, radians per second.
    pub turn_rate: f32,
    /// Turn speed standing, radians per second ([`SPOT_TURN_RATE`]).
    pub spot_turn_rate: f32,
    /// Standing, a way to walk further than this off the heading (radians) is turned to
    /// on the spot before setting off ([`SPOT_TURN_START`]).
    pub spot_turn_start: f32,
    /// Turning on the spot before walking on.
    pub turning: bool,
    /// Moved in the last update (a walker that did not is standing).
    pub moving: bool,
    pub follower: Option<Follower>,
    /// Horizontal distance moved so far.
    pub travelled: f32,
    /// In the air over a ledge link ([`crate::jump`]): the arc and the time since take-off.
    pub air: Option<Air>,
    /// Ledges jumped (taken off over) and landings, so far.
    pub flights: u32,
    pub landings: u32,
    /// How far its heading was off the door (radians) when it last went through a load
    /// door whose place is known: 0 once it has turned to it ([`Walker::face_door`]).
    pub door_off: Option<f32>,
}

/// How far from a ledge link's take-off point the feet may be for the flight to leave
/// from that point, game units [design: about a frame's step at a run].
pub const TAKE_OFF_SNAP: f32 = 16.0;

/// A [`Walker`]'s flight over a ledge link.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Air {
    pub flight: Flight,
    pub t: f32,
}

/// Fastest turn of a walking actor, radians per second (180 degrees per second)
/// [design; owner's playtest 2026-10-11: actors turned half about in a frame or two].
/// The game's characters turn at the setting `fCharacterDefaultTurningSpeed` [documented:
/// GECK wiki, Settings list], whose value is not in FalloutNV.esm (a built-in default);
/// creatures have their own, CREA `TNAM` "Turning Speed" [documented: xEdit], degrees per
/// second × 1.25 on big turns [observed, actors.md §8.10 "TNAM"], given to their walkers
/// by the engine (`creature_pace::creature_turn_rate`). Was 720 degrees per second.
pub const WALK_TURN_RATE: f32 = std::f32::consts::PI;

/// While its heading catches up with the way to go, a walking actor keeps this share of
/// its speed at the least (heading a quarter turn or more off) and all of it when facing
/// the way ([`turn_slowdown`]) [design].
pub const TURN_SLOW_MIN: f32 = 0.3;

/// The share of its speed a walking actor keeps when its heading is `off` radians from
/// the way to go: it slows to turn [design].
pub fn turn_slowdown(off: f32) -> f32 {
    TURN_SLOW_MIN + (1.0 - TURN_SLOW_MIN) * off.cos().max(0.0)
}

/// The direction a heading faces (0 north = +y, π/2 east = +x).
pub fn dir_of(heading: f32) -> [f32; 2] {
    let (s, c) = heading.sin_cos();
    [s, c]
}

/// Which way a walking actor moves while it turns towards `way` (a unit direction) with
/// its heading now at `heading`: along its heading, so that it walks an arc round a
/// corner, while that is within a quarter turn of the way; else along the way itself
/// (it does not walk off backwards) [design]. With the share of its speed it keeps
/// ([`turn_slowdown`]).
pub fn arc_step(heading: f32, way: [f32; 2]) -> ([f32; 2], f32) {
    let off = heading_delta(heading, heading_of(way)).abs();
    let along = if off < std::f32::consts::FRAC_PI_2 { dir_of(heading) } else { way };
    (along, turn_slowdown(off))
}

/// Turn speed of a standing actor, radians per second (120 degrees per second) [design:
/// the turn clips carry no rotation, docs/spec-notes/actors.md §8.2 and §8.9].
pub const SPOT_TURN_RATE: f32 = 120.0 * std::f32::consts::PI / 180.0;
/// A standing actor turns on the spot before walking when its way lies further than
/// this off its heading, radians (20 degrees) [design, actors.md §8.9].
pub const SPOT_TURN_START: f32 = 20.0 * std::f32::consts::PI / 180.0;

/// Compass heading of a horizontal direction (0 north = +y, π/2 east = +x).
pub fn heading_of(dir: [f32; 2]) -> f32 {
    dir[0].atan2(dir[1])
}

/// Turn `from` towards `to` (radians) by at most `max`, the short way round.
pub fn turn_towards(from: f32, to: f32, max: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    let d = (to - from + PI).rem_euclid(TAU) - PI;
    let r = from + d.clamp(-max, max);
    (r + PI).rem_euclid(TAU) - PI
}

/// The turn from heading `from` to `to`, radians in [-π, π): positive is clockwise seen
/// from above (to the right), as compass headings grow.
pub fn heading_delta(from: f32, to: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (to - from + PI).rem_euclid(TAU) - PI
}

/// Headings closer than this count as the same (radians).
const FACING: f32 = 1e-4;

impl Walker {
    pub fn new(space: Space, feet: [f32; 3], heading: f32, speed: f32) -> Walker {
        Walker {
            space,
            feet,
            heading,
            tri: NONE,
            speed,
            pace: 1.0,
            turn_rate: WALK_TURN_RATE,
            spot_turn_rate: SPOT_TURN_RATE,
            spot_turn_start: SPOT_TURN_START,
            turning: false,
            moving: false,
            follower: None,
            travelled: 0.0,
            air: None,
            flights: 0,
            landings: 0,
            door_off: None,
        }
    }

    /// The speed it moves at: [`Self::speed`] times [`Self::pace`].
    pub fn moving_speed(&self) -> f32 {
        self.speed * self.pace
    }

    /// Turn on the spot towards `heading` for `dt` seconds, at the standing turn speed.
    /// Returns whether the walker faces it now. For an actor standing with nowhere to
    /// go (facing whoever it talks to, say); on a path the walker turns by itself.
    pub fn face(&mut self, heading: f32, dt: f32) -> bool {
        self.heading = turn_towards(self.heading, heading, self.spot_turn_rate * dt.max(0.0));
        heading_delta(self.heading, heading).abs() <= FACING
    }

    /// At load door `d` (given [`Steer::Door`]) with the feet at `feet`: stand and turn on
    /// the spot to face the door itself before going through, rather than carry on into
    /// it along the way the actor came, which from the side walked it past the frame
    /// (#1472) [design: how the game turns actors at doors is not measured]. Returns
    /// whether it is still turning; once it faces the door (or the door's place is not
    /// known) the door is used.
    pub fn face_door(&mut self, d: &DoorEdge, feet: [f32; 3], dt: f32) -> bool {
        let Some(at) = d.door_at else { return false };
        let dir = [at[0] - feet[0], at[1] - feet[1]];
        if dir[0].hypot(dir[1]) < 1.0 {
            return false;
        }
        let want = heading_of(dir);
        if heading_delta(self.heading, want).abs() <= FACING {
            return false;
        }
        self.face(want, dt);
        true
    }

    /// Plan a path from the walker's feet to `to` and start following it.
    pub fn go_to(&mut self, nav: &NavGraph, to: Place, params: &PathParams) -> Result<&Path, PathError> {
        let path = nav.find_path(Place { space: self.space, pos: self.feet }, to, params)?;
        self.follower = Some(Follower::new(path));
        Ok(&self.follower.as_ref().expect("just set").path)
    }

    /// The way from the feet to where the path led ([`WalkEvent::PathLost`], `params` as
    /// for a walk): the walker keeps its path when there is none. Returns its length.
    pub fn replan(&mut self, nav: &NavGraph, params: &PathParams) -> Result<f32, PathError> {
        let old = self.follower.as_ref().ok_or(PathError::Unreachable { expanded: 0 })?;
        let f = old.replanned(nav, Place { space: self.space, pos: self.feet }, params)?;
        let length = f.path.length;
        self.follower = Some(f);
        self.turning = false;
        Ok(length)
    }

    /// Move for `dt` seconds, passing doors that open in place as if they were open.
    pub fn update(&mut self, nav: &NavGraph, dt: f32) -> WalkEvent {
        self.update_with(nav, dt, &mut |_| true)
    }

    /// Move for `dt` seconds. At a door that opens in place, `open(door)` is asked every
    /// update until it says the door is fully open: it should start the door opening (if
    /// it is not already) and answer whether its "Open" sequence has ended; the walker
    /// stands meanwhile.
    pub fn update_with(&mut self, nav: &NavGraph, dt: f32, open: &mut dyn FnMut(u32) -> bool) -> WalkEvent {
        let Some(mut f) = self.follower.take() else { return WalkEvent::Idle };
        let ev = self.advance(nav, &mut f, dt, open);
        self.moving = ev == WalkEvent::Moved;
        // Keep the path unless it ended (an idle update with the path not finished keeps it).
        let keep = match ev {
            WalkEvent::Arrived => false,
            WalkEvent::Idle => f.steer(self.feet) != Steer::Arrived,
            WalkEvent::Moved | WalkEvent::Turned | WalkEvent::UsedDoor(_) | WalkEvent::WaitingForDoor(_) | WalkEvent::PathLost => true,
        };
        if !keep {
            self.turning = false;
        }
        if keep {
            self.follower = Some(f);
        }
        ev
    }

    /// Fly on over a ledge for `dt` seconds; on landing the feet are on the landing
    /// point (where the path goes on from).
    fn fly(&mut self, nav: &NavGraph, dt: f32) -> WalkEvent {
        let Some(mut a) = self.air else { return WalkEvent::Idle };
        let before = self.feet;
        a.t += dt.max(0.0);
        self.feet = a.flight.at(a.t);
        self.travelled += hdist(before, self.feet);
        if a.t >= a.flight.duration {
            self.air = None;
            self.landings += 1;
            self.tri = self.tri_near(nav, self.feet).unwrap_or(NONE);
        } else {
            self.air = Some(a);
        }
        WalkEvent::Moved
    }

    fn advance(&mut self, nav: &NavGraph, f: &mut Follower, dt: f32, open: &mut dyn FnMut(u32) -> bool) -> WalkEvent {
        if self.air.is_some() {
            return self.fly(nav, dt);
        }
        let mut budget = self.moving_speed() * dt;
        let mut moved = false;
        let mut settled = false;
        // A step may pass several close corners.
        for _ in 0..8 {
            match f.steer(self.feet) {
                Steer::Arrived => return if moved { WalkEvent::Arrived } else { WalkEvent::Idle },
                Steer::OffPath => {
                    if moved {
                        return WalkEvent::Moved;
                    }
                    // This mover keeps to the navmesh: feet it did not put there (placed
                    // off it, left by physics) go onto the surface under them first, as its
                    // next step would put them; where the navmesh has nothing under them,
                    // the path's height, as its step does there. If that fits, it walks on;
                    // if they are on another floor of the navmesh they stay on it.
                    if !settled {
                        settled = true;
                        let z = match self.tri_near(nav, self.feet) {
                            Some(t) => {
                                self.tri = t;
                                Some(nav.height_on(t, self.feet[0], self.feet[1]))
                            }
                            None => f.path.legs.get(f.leg).and_then(|l| l.points.get(f.next.min(l.points.len().saturating_sub(1)))).map(|p| p[2]),
                        };
                        if let Some(z) = z.filter(|z| *z != self.feet[2]) {
                            self.feet[2] = z;
                            continue;
                        }
                    }
                    return WalkEvent::PathLost;
                }
                Steer::OpenDoor { door, .. } => {
                    if open(door) {
                        f.door_opened();
                        continue;
                    }
                    return WalkEvent::WaitingForDoor(door);
                }
                Steer::Door(d) => {
                    // It stops at the door and faces it (after this step's walk, on
                    // the next update) before it goes through.
                    if moved && d.door_at.is_some() {
                        return WalkEvent::Moved;
                    }
                    if self.face_door(&d, self.feet, dt) {
                        self.turning = false;
                        return WalkEvent::Turned;
                    }
                    self.door_off = d.door_at.map(|at| heading_delta(self.heading, heading_of([at[0] - self.feet[0], at[1] - self.feet[1]])).abs());
                    f.door_used();
                    self.space = f.space().unwrap_or(self.space);
                    self.feet = d.arrival;
                    self.heading = d.arrival_heading;
                    self.tri = d.to;
                    return WalkEvent::UsedDoor(d);
                }
                Steer::Walk { target, dir, to_target, .. } => {
                    if budget <= 1e-4 {
                        break;
                    }
                    // At the take-off of a ledge link: over it in an arc to the landing.
                    if let Some((hop, landing)) = f.hop() {
                        if dir != [0.0, 0.0] {
                            self.heading = heading_of(dir);
                        }
                        self.turning = false;
                        // From the link's own take-off point when the feet are at it, so
                        // where the actor leaves does not hang on its speed and the frame's
                        // step (the feet stand within a step past it) [design].
                        if let Some(t) = f.take_off().filter(|t| hdist(*t, self.feet) <= TAKE_OFF_SNAP && height_fits(self.feet[2], t[2], f.height_offset)) {
                            self.feet = t;
                        }
                        self.air = Some(Air { flight: Flight::over_ledge(self.feet, landing, hop.up), t: 0.0 });
                        self.flights += 1;
                        let left = budget / self.moving_speed().max(1e-4);
                        return self.fly(nav, left);
                    }
                    // Standing and facing away from the way to go: turn on the spot
                    // first (the turn clips play, actors.md §8.9), then set off.
                    let want = heading_of(dir);
                    let standing = !self.moving && !moved;
                    if dir != [0.0, 0.0] && (self.turning || (standing && heading_delta(self.heading, want).abs() > self.spot_turn_start)) {
                        self.turning = !self.face(want, dt);
                        return WalkEvent::Turned;
                    }
                    self.heading = turn_towards(self.heading, want, self.turn_rate * dt);
                    // It slows while its heading catches up (on the navmesh it keeps
                    // to the path itself; the character controller walks the arc).
                    let slow = turn_slowdown(heading_delta(self.heading, want).abs());
                    let step = (budget * slow).min(to_target);
                    let start = self.feet;
                    let mut p = [start[0] + dir[0] * step, start[1] + dir[1] * step, start[2]];
                    // Height: the navmesh under the new point, else along the segment.
                    let near = self.tri_near(nav, p);
                    p[2] = match near {
                        Some(t) => {
                            self.tri = t;
                            nav.height_on(t, p[0], p[1])
                        }
                        None => start[2] + (target[2] - start[2]) * (step / to_target.max(1e-4)),
                    };
                    self.feet = p;
                    self.travelled += step;
                    budget -= step / slow;
                    moved = true;
                }
            }
        }
        if moved {
            WalkEvent::Moved
        } else {
            WalkEvent::Idle
        }
    }

    /// The triangle under `p`: the current one or one of its neighbours (two rings),
    /// else a search of the space.
    fn tri_near(&self, nav: &NavGraph, p: [f32; 3]) -> Option<u32> {
        if self.tri != NONE {
            let mut ring = vec![self.tri];
            for _ in 0..2 {
                if let Some(&t) = ring.iter().find(|&&t| nav.contains_xy(t, p[0], p[1])) {
                    return Some(t);
                }
                let mut next = Vec::new();
                for &t in &ring {
                    next.extend(nav.tris[t as usize].next.iter().copied().filter(|&n| n != NONE));
                }
                ring = next;
            }
            if let Some(&t) = ring.iter().find(|&&t| nav.contains_xy(t, p[0], p[1])) {
                return Some(t);
            }
        }
        nav.locate(self.space, p, 128.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::Leg;

    fn path(points: Vec<[f32; 3]>) -> Path {
        Path { legs: vec![Leg { space: Space::Interior(1), points, corridor: vec![], door: None, door_stops: vec![], hops: vec![] }], length: 0.0, expanded: 0, start_snap: 0.0, goal_snap: 0.0 }
    }

    #[test]
    fn follower_walks_corners_in_order() {
        let mut f = Follower::new(path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]]));
        match f.steer([0.0, 0.0, 0.0]) {
            Steer::Walk { target, dir, remaining, .. } => {
                assert_eq!(target, [100.0, 0.0, 0.0]);
                assert_eq!(dir, [1.0, 0.0]);
                assert!((remaining - 200.0).abs() < 1e-3);
            }
            s => panic!("{s:?}"),
        }
        // Within the arrival radius of the corner: head for the end.
        match f.steer([95.0, 0.0, 0.0]) {
            Steer::Walk { target, .. } => assert_eq!(target, [100.0, 100.0, 0.0]),
            s => panic!("{s:?}"),
        }
        assert_eq!(f.steer([100.0, 99.0, 0.0]), Steer::Arrived);
    }

    #[test]
    fn passing_a_corner_counts() {
        let mut f = Follower::new(path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]]));
        // Overshot the corner by 30 units, 30 units from it.
        match f.steer([130.0, 0.0, 0.0]) {
            Steer::Walk { target, .. } => assert_eq!(target, [100.0, 100.0, 0.0]),
            s => panic!("{s:?}"),
        }
    }

    #[test]
    fn unstuck_detours_then_replans_then_gives_up() {
        let mut u = Unstuck::new(100.0);
        u.max_replans = 1;
        let dt = 0.1;
        // Moving well: no advice beyond the path's direction.
        for i in 0..20 {
            assert_eq!(u.advise(1000.0 - i as f32 * 10.0, [1.0, 0.0], dt), Advice::Head([1.0, 0.0]));
        }
        // Blocked 800 from the end: after a second, the four detours in turn, each 0.6 s.
        let mut turns = Vec::new();
        let mut advice = Vec::new();
        for _ in 0..200 {
            let a = u.advise(800.0, [1.0, 0.0], dt);
            if let Advice::Head(d) = a {
                if d != [1.0, 0.0] && turns.last() != Some(&d) {
                    turns.push(d);
                }
            } else {
                advice.push(a);
            }
            if advice.contains(&Advice::GiveUp) {
                break;
            }
        }
        assert_eq!(turns.len(), 8, "{turns:?}"); // four detours, twice (before and after the replan)
        assert!(turns[0][1] > 0.9 && turns[1][1] < -0.9, "left first, then right: {turns:?}");
        assert_eq!(advice, vec![Advice::Replan, Advice::GiveUp]);
        assert_eq!((u.detours, u.replans), (8, 1));
    }

    #[test]
    fn unstuck_with_skip_advises_skip_instead_of_replan() {
        let mut u = Unstuck::new(100.0);
        u.skip = true;
        let mut advice = Vec::new();
        for _ in 0..200 {
            match u.advise(800.0, [1.0, 0.0], 0.1) {
                Advice::Head(_) => {}
                a => {
                    advice.push(a);
                    break;
                }
            }
        }
        assert_eq!(advice, vec![Advice::Skip]);
        assert_eq!((u.detours, u.replans), (4, 1));
    }

    #[test]
    fn after_a_skip_a_still_stuck_actor_is_moved_on_again_at_once() {
        // Stuck for good: the first skip comes after a second and four detours.
        let first = |u: &mut Unstuck| (1..400).find(|_| u.advise(800.0, [1.0, 0.0], 0.1) == Advice::Skip).unwrap();
        let mut u = Unstuck::new(100.0);
        u.skip = true;
        let steps = first(&mut u);
        assert!(u.skipped && steps > 60, "{steps}");
        // Put down further on and stuck again: moved on after a second without progress.
        let mut again = Unstuck::new(100.0);
        again.skip = true;
        again.after_skip();
        let steps = first(&mut again);
        assert!(steps <= 11 && again.detours == 0, "{steps} {}", again.detours);
        // Getting on after the skip brings the detours back.
        let mut free = Unstuck::new(100.0);
        free.skip = true;
        free.after_skip();
        for i in 0..20 {
            assert_eq!(free.advise(800.0 - i as f32 * 10.0, [1.0, 0.0], 0.1), Advice::Head([1.0, 0.0]));
        }
        let steps = first(&mut free);
        assert!(free.detours == 4 && steps > 60, "{steps} {}", free.detours);
    }

    #[test]
    fn unstuck_by_motion_is_not_fooled_by_a_receding_target() {
        let mut u = Unstuck::new(100.0);
        // Walking after a target that keeps 500 ahead: moving at full speed is progress.
        for i in 0..100 {
            assert_eq!(u.advise_moved([i as f32 * 10.0, 0.0, 0.0], [1.0, 0.0], 0.1), Advice::Head([1.0, 0.0]));
        }
        // Sliding across the way (along a fence) is not.
        let mut detoured = false;
        for i in 0..30 {
            if u.advise_moved([1000.0, i as f32 * 10.0, 0.0], [1.0, 0.0], 0.1) != Advice::Head([1.0, 0.0]) {
                detoured = true;
            }
        }
        assert!(detoured);
        assert!(u.detours >= 1);
    }

    #[test]
    fn skip_ahead_moves_on_along_the_path() {
        let mut f = Follower::new(path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 10.0], [100.0, 300.0, 40.0]]));
        // Stuck 60 units along the first stretch: 160 on is 40 units past the corner,
        // which counts as reached; the height is the path's.
        let to = f.skip_ahead([60.0, 0.0, 6.0], 160.0).unwrap();
        assert!((to[0] - 100.0).abs() < 1e-3 && (to[1] - 120.0).abs() < 1e-3 && (to[2] - 22.0).abs() < 1e-3, "{to:?}");
        assert_eq!(f.next, 2);
        // Near the end: no further than the end, which then counts as arrived at.
        let to = f.skip_ahead([100.0, 250.0, 35.0], 160.0).unwrap();
        assert_eq!(to, [100.0, 300.0, 40.0]);
        assert_eq!(f.steer(to), Steer::Arrived);
    }

    #[test]
    fn skip_ahead_stops_at_a_door_stop() {
        let mut p = path(vec![[0.0, 0.0, 0.0], [50.0, 0.0, 0.0], [400.0, 0.0, 0.0]]);
        p.legs[0].door_stops = vec![crate::path::DoorStop { door: 7, point: 1 }];
        let mut f = Follower::new(p);
        let to = f.skip_ahead([10.0, 0.0, 0.0], 160.0).unwrap();
        assert_eq!(to, [50.0, 0.0, 0.0]);
        assert!(matches!(f.steer(to), Steer::OpenDoor { door: 7, .. }));
        // Held at the stop: no skipping through the door.
        assert_eq!(f.skip_ahead(to, 160.0), None);
    }

    #[test]
    fn turning_takes_the_short_way() {
        use std::f32::consts::PI;
        let h = turn_towards(0.9 * PI, -0.9 * PI, 0.1);
        assert!((h - (0.9 * PI + 0.1)).abs() < 1e-5 || (h - (0.9 * PI + 0.1 - 2.0 * PI)).abs() < 1e-5, "{h}");
        assert!((turn_towards(0.0, 0.05, 1.0) - 0.05).abs() < 1e-6);
        assert!((heading_of([1.0, 0.0]) - PI / 2.0).abs() < 1e-6);
        assert!(heading_of([0.0, 1.0]).abs() < 1e-6);
        assert!((heading_delta(0.9 * PI, -0.9 * PI) - 0.2 * PI).abs() < 1e-5);
        assert!((heading_delta(0.1, -0.1) + 0.2).abs() < 1e-6);
    }

    /// A walker on a path with no navmesh under it (heights come from the path).
    fn walker(heading: f32, points: Vec<[f32; 3]>) -> (NavGraph, Walker) {
        let mut w = Walker::new(Space::Interior(1), points[0], heading, 100.0);
        w.follower = Some(Follower::new(path(points)));
        (NavGraph::default(), w)
    }

    #[test]
    fn a_standing_walker_turns_on_the_spot_before_it_sets_off() {
        use std::f32::consts::{FRAC_PI_2, PI};
        // Facing north, the way lies east: a quarter turn to the right at 120°/s.
        let (g, mut w) = walker(0.0, vec![[0.0, 0.0, 0.0], [200.0, 0.0, 0.0]]);
        let dt = 0.05;
        let mut turned = 0;
        loop {
            let before = w.heading;
            match w.update(&g, dt) {
                WalkEvent::Turned => {
                    turned += 1;
                    assert_eq!(w.feet, [0.0, 0.0, 0.0], "it stands while it turns");
                    let step = heading_delta(before, w.heading);
                    assert!(step > 0.0 && step <= SPOT_TURN_RATE * dt + 1e-5, "{step}");
                }
                WalkEvent::Moved => break,
                e => panic!("{e:?}"),
            }
            assert!(turned < 100);
        }
        // 90° at 120°/s is 0.75 s: fifteen steps of 0.05 s (one more for rounding).
        assert!((15..=16).contains(&turned), "{turned}");
        assert!((w.heading - FRAC_PI_2).abs() < 1e-4 && !w.turning && w.moving);
        // The other way round it turns left.
        let (g, mut w) = walker(PI, vec![[0.0, 0.0, 0.0], [200.0, 0.0, 0.0]]);
        assert_eq!(w.update(&g, dt), WalkEvent::Turned);
        assert!(heading_delta(PI, w.heading) < 0.0);
        // Nearly facing the way (within 20°): it sets off at once.
        let (g, mut w) = walker(FRAC_PI_2 - 0.3, vec![[0.0, 0.0, 0.0], [200.0, 0.0, 0.0]]);
        assert_eq!(w.update(&g, dt), WalkEvent::Moved);
        // It turns on as it walks, at the walking rate.
        assert!((w.heading - (FRAC_PI_2 - 0.3 + WALK_TURN_RATE * dt)).abs() < 1e-4, "{}", w.heading);
    }

    #[test]
    fn a_walker_on_its_way_takes_corners_without_stopping() {
        // East, then a right angle north: walking, it steers round the corner.
        let (g, mut w) = walker(std::f32::consts::FRAC_PI_2, vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0]]);
        let mut events = Vec::new();
        for _ in 0..100 {
            let ev = w.update(&g, 0.05);
            events.push(ev);
            if ev == WalkEvent::Arrived {
                break;
            }
        }
        assert_eq!(events.last(), Some(&WalkEvent::Arrived));
        assert!(!events.contains(&WalkEvent::Turned), "{events:?}");
        // (It cuts the corner within the arrival radius, so it ends a little east of north.)
        assert!(w.heading.abs() < 0.2 && !w.moving, "{}", w.heading);
    }

    #[test]
    fn a_walker_turns_at_most_at_the_walking_rate_and_slows_to_turn() {
        use std::f32::consts::FRAC_PI_2;
        // East, then a right angle north, at 100 units per second in steps of 1/60 s.
        let (g, mut w) = walker(FRAC_PI_2, vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 200.0, 0.0]]);
        let dt = 1.0 / 60.0;
        let mut slowest = f32::MAX;
        for _ in 0..600 {
            let (h, at) = (w.heading, w.feet);
            if w.update(&g, dt) == WalkEvent::Arrived {
                break;
            }
            assert!(heading_delta(h, w.heading).abs() <= WALK_TURN_RATE * dt + 1e-5, "turned {} in a step", heading_delta(h, w.heading));
            slowest = slowest.min((w.feet[0] - at[0]).hypot(w.feet[1] - at[1]) / dt);
        }
        // Round the corner it slowed down (it turns a quarter in half a second).
        assert!(slowest < 80.0, "{slowest}");
        assert!(w.follower.is_none() && (w.feet[1] - 200.0).abs() < 20.0, "{:?}", w.feet);
    }

    #[test]
    fn the_arc_follows_the_heading_within_a_quarter_turn() {
        use std::f32::consts::{FRAC_PI_2, PI};
        // Facing north, the way east: half a quarter off, it goes along its heading.
        let (d, k) = arc_step(PI / 4.0, [1.0, 0.0]);
        assert!((d[0] - (PI / 4.0).sin()).abs() < 1e-6 && (d[1] - (PI / 4.0).cos()).abs() < 1e-6);
        assert!(k > TURN_SLOW_MIN && k < 1.0);
        // Facing the way: full speed. Facing away: along the way, slowest.
        assert_eq!(arc_step(FRAC_PI_2, [1.0, 0.0]).1, 1.0);
        let (d, k) = arc_step(-FRAC_PI_2, [1.0, 0.0]);
        assert_eq!((d, k), ([1.0, 0.0], TURN_SLOW_MIN));
    }

    #[test]
    fn facing_a_heading_standing() {
        use std::f32::consts::PI;
        let (_, mut w) = walker(0.0, vec![[0.0, 0.0, 0.0], [200.0, 0.0, 0.0]]);
        w.follower = None;
        // Half a turn at 120°/s: 1.5 s.
        let mut t = 0.0f32;
        while !w.face(PI, 0.1) {
            t += 0.1;
            assert!(t < 3.0);
        }
        assert!((t - 1.4).abs() < 0.11, "{t}");
        assert!(heading_delta(w.heading, PI).abs() < 1e-4);
    }

    #[test]
    fn a_walker_flies_over_a_ledge_and_lands_on_the_path() {
        // A drop of 300 between (100, 0) and (200, 0): the path's hop.
        let mut p = path(vec![[0.0, 0.0, 300.0], [100.0, 0.0, 300.0], [200.0, 0.0, 0.0], [400.0, 0.0, 0.0]]);
        p.legs[0].hops = vec![LedgeHop { point: 1, up: false }];
        let mut w = Walker::new(Space::Interior(1), [0.0, 0.0, 300.0], std::f32::consts::FRAC_PI_2, 100.0);
        w.follower = Some(Follower::new(p));
        let g = NavGraph::default();
        let dt = 1.0 / 60.0;
        let (mut in_air, mut lowest_step) = (0, 0.0f32);
        for _ in 0..600 {
            let z = w.feet[2];
            if w.update(&g, dt) == WalkEvent::Arrived {
                break;
            }
            if w.air.is_some() {
                in_air += 1;
                // Falling faster and faster: never a ramp.
                lowest_step = lowest_step.min(w.feet[2] - z);
                assert!(w.feet[0] > 99.0 && w.feet[0] < 201.0, "{:?}", w.feet);
            }
        }
        assert_eq!((w.flights, w.landings), (1, 1));
        // 300 down from a standing start: 0.935 s, 56 steps at 60 a second.
        assert!((54..=57).contains(&in_air), "{in_air}");
        assert!(lowest_step < -9.0, "{lowest_step}");
        assert!(w.follower.is_none() && (w.feet[0] - 400.0).abs() < 2.0 && w.feet[2] == 0.0, "{:?}", w.feet);
    }

    #[test]
    fn a_walker_jumps_up_a_low_ledge() {
        let mut p = path(vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [130.0, 0.0, 50.0], [300.0, 0.0, 50.0]]);
        p.legs[0].hops = vec![LedgeHop { point: 1, up: true }];
        let mut w = Walker::new(Space::Interior(1), [0.0, 0.0, 0.0], std::f32::consts::FRAC_PI_2, 100.0);
        w.follower = Some(Follower::new(p));
        let g = NavGraph::default();
        let mut top = 0.0f32;
        for _ in 0..600 {
            if w.update(&g, 1.0 / 60.0) == WalkEvent::Arrived {
                break;
            }
            top = top.max(w.feet[2]);
        }
        // Above the ledge (50) by at least the jump's apex (61.6) or the clearance.
        assert!(top > 58.0 && top < 62.0, "{top}");
        assert_eq!((w.flights, w.landings), (1, 1));
        assert_eq!(w.feet, [300.0, 0.0, 50.0]);
    }
}
