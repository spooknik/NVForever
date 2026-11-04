//! Graph, path and walker tests on synthetic navmeshes (no game data).

use crate::graph::{EdgeKind, NavGraph, Space, NONE};
use crate::path::{PathError, PathParams, Place};
use crate::record::{link_kind, tri_flags, DoorLink, EdgeLink, NavMeshRecord, Triangle};
use crate::follow::{PathRecovery, HEIGHT_TOLERANCE, RECOVERY_RESET_DISTANCE, RECOVERY_RESET_SECONDS, RECOVERY_TRIES};
use crate::{Follower, Steer, WalkEvent, Walker};
use std::collections::HashSet;

const W: Space = Space::World(0x3C);

/// A navmesh over a grid of `nx` × `ny` squares of side `size` from `origin`, each
/// square two counter-clockwise triangles (south-east and north-west of its diagonal),
/// except the squares in `holes`. Heights follow `z(x, y)`.
fn grid(form_id: u32, origin: [f32; 2], nx: usize, ny: usize, size: f32, holes: &[(usize, usize)], z: impl Fn(f32, f32) -> f32) -> NavMeshRecord {
    let vid = |i: usize, j: usize| (j * (nx + 1) + i) as u16;
    let mut vertices = Vec::new();
    for j in 0..=ny {
        for i in 0..=nx {
            let (x, y) = (origin[0] + i as f32 * size, origin[1] + j as f32 * size);
            vertices.push([x, y, z(x, y)]);
        }
    }
    let holes: HashSet<(usize, usize)> = holes.iter().copied().collect();
    // Triangle index of (square, half): half 0 = (a, b, c), half 1 = (a, c, d).
    let mut index = std::collections::HashMap::new();
    for j in 0..ny {
        for i in 0..nx {
            if !holes.contains(&(i, j)) {
                for h in 0..2 {
                    let n = index.len() as i16;
                    index.insert((i, j, h), n);
                }
            }
        }
    }
    let at = |i: isize, j: isize, h: usize| -> i16 {
        if i < 0 || j < 0 {
            return -1;
        }
        index.get(&(i as usize, j as usize, h)).copied().unwrap_or(-1)
    };
    let mut triangles = vec![Triangle { v: [0; 3], edges: [-1; 3], flags: 0, cover: 0 }; index.len()];
    for (&(i, j, h), &n) in &index {
        let (a, b, c, d) = (vid(i, j), vid(i + 1, j), vid(i + 1, j + 1), vid(i, j + 1));
        let (ii, jj) = (i as isize, j as isize);
        triangles[n as usize] = if h == 0 {
            Triangle { v: [a, b, c], edges: [at(ii, jj - 1, 1), at(ii + 1, jj, 1), at(ii, jj, 1)], flags: 0, cover: 0 }
        } else {
            Triangle { v: [a, c, d], edges: [at(ii, jj, 0), at(ii, jj + 1, 0), at(ii - 1, jj, 0)], flags: 0, cover: 0 }
        };
    }
    NavMeshRecord { form_id, vertices, triangles, ..Default::default() }
}

/// Link `a`'s east boundary edges to `b`'s west boundary edges (and back) where they
/// overlap, as NVEX portals.
fn seam(a: &mut NavMeshRecord, b: &mut NavMeshRecord, kind: u32) {
    let ax = a.vertices.iter().map(|v| v[0]).fold(f32::MIN, f32::max);
    let east: Vec<(usize, usize, [f32; 3], [f32; 3])> = edges_at(a, ax);
    let west = edges_at(b, ax);
    for &(at, ae, p0, p1) in &east {
        let (lo, hi) = (p0[1].min(p1[1]), p0[1].max(p1[1]));
        let mid = (lo + hi) / 2.0;
        if let Some(&(bt, be, ..)) = west.iter().find(|w| w.2[1].min(w.3[1]) <= mid && mid <= w.2[1].max(w.3[1])) {
            link(a, at, ae, b.form_id, bt, kind);
            link(b, bt, be, a.form_id, at, kind);
        }
    }
}

/// Boundary edges of `m` lying on the line x = `x`: (triangle, edge, ends).
fn edges_at(m: &NavMeshRecord, x: f32) -> Vec<(usize, usize, [f32; 3], [f32; 3])> {
    let mut out = Vec::new();
    for (ti, t) in m.triangles.iter().enumerate() {
        for e in 0..3 {
            let (p, q) = (m.vertices[t.v[e] as usize], m.vertices[t.v[(e + 1) % 3] as usize]);
            if t.edges[e] == -1 && (p[0] - x).abs() < 1e-3 && (q[0] - x).abs() < 1e-3 {
                out.push((ti, e, p, q));
            }
        }
    }
    out
}

fn link(m: &mut NavMeshRecord, t: usize, e: usize, to: u32, tt: usize, kind: u32) {
    m.triangles[t].flags |= 1 << e;
    m.triangles[t].edges[e] = m.edge_links.len() as i16;
    m.edge_links.push(EdgeLink { kind, navmesh: Some(to), triangle: tt as u16 });
}

fn graph(meshes: &[(NavMeshRecord, Space)]) -> NavGraph {
    let input: Vec<_> = meshes.iter().map(|(m, s)| (m, Some(0), Some(*s))).collect();
    NavGraph::assemble(&input)
}

fn flat(_: f32, _: f32) -> f32 {
    0.0
}

/// Every point along the polyline (every `step` units) has navmesh under it.
fn on_mesh(g: &NavGraph, space: Space, points: &[[f32; 3]], step: f32) -> bool {
    points.windows(2).all(|w| {
        let n = ((w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]) / step).ceil().max(1.0) as usize;
        (0..=n).all(|k| {
            let s = k as f32 / n as f32;
            let p = [w[0][0] + (w[1][0] - w[0][0]) * s, w[0][1] + (w[1][1] - w[0][1]) * s, w[0][2] + (w[1][2] - w[0][2]) * s];
            g.locate(space, p, 1000.0).is_some()
        })
    })
}

#[test]
fn internal_edges_resolve_and_outline_is_marked() {
    let g = graph(&[(grid(1, [0.0, 0.0], 3, 2, 100.0, &[], flat), W)]);
    assert_eq!(g.tris.len(), 12);
    assert_eq!(g.stats.internal_edges, 12 * 3 - 10); // 36 triangle edges, 10 on the outline
    assert_eq!(g.stats.boundary_edges, 10);
    // Every internal neighbour points back.
    for (i, t) in g.tris.iter().enumerate() {
        for e in 0..3 {
            if t.kind[e] == EdgeKind::Internal {
                assert!(g.tris[t.next[e] as usize].next.contains(&(i as u32)));
            }
        }
    }
    // The centre vertex (1, 1) of the grid is inside; the corner (0, 0) on the outline.
    assert!(g.boundary_vertex[0]);
    assert!(!g.boundary_vertex[4 + 1]);
    let t = g.locate(W, [150.0, 50.0, 10.0], 100.0).expect("triangle under the point");
    assert!(g.contains_xy(t, 150.0, 50.0));
    assert_eq!(g.locate(W, [350.0, 50.0, 0.0], 100.0), None);
}

#[test]
fn straight_path_in_the_open() {
    let g = graph(&[(grid(1, [0.0, 0.0], 10, 10, 100.0, &[], flat), W)]);
    let p = g.find_path(Place { space: W, pos: [150.0, 150.0, 0.0] }, Place { space: W, pos: [850.0, 750.0, 0.0] }, &PathParams::default()).unwrap();
    assert_eq!(p.legs.len(), 1);
    assert_eq!(p.legs[0].points.len(), 2, "{:?}", p.legs[0].points);
    assert!((p.length - (700.0f32).hypot(600.0)).abs() < 1.0);
}

#[test]
fn path_goes_round_a_wall_with_clearance() {
    // A wall of holes across x = 400..500 from y = 0 to 800; the way round is at the north.
    let holes: Vec<(usize, usize)> = (0..8).map(|j| (4, j)).collect();
    let g = graph(&[(grid(1, [0.0, 0.0], 10, 10, 100.0, &holes, flat), W)]);
    let params = PathParams { radius: 24.0, ..Default::default() };
    let p = g.find_path(Place { space: W, pos: [150.0, 150.0, 0.0] }, Place { space: W, pos: [850.0, 150.0, 0.0] }, &params).unwrap();
    let pts = &p.legs[0].points;
    assert!(pts.len() >= 4, "{pts:?}");
    assert!(on_mesh(&g, W, pts, 5.0), "{pts:?}");
    // Every corner keeps the clearance from the wall (x 400-500, y 0-800).
    for c in &pts[1..pts.len() - 1] {
        let dx = (400.0 - c[0]).max(c[0] - 500.0).max(0.0);
        let dy = (c[1] - 800.0).max(0.0);
        assert!(dx.hypot(dy) >= 23.9, "corner too close to the wall: {c:?} in {pts:?}");
    }
    // Rounding the wall's end, the path never turns back west.
    assert!(pts.windows(2).all(|w| w[1][0] >= w[0][0] - 1e-3), "{pts:?}");
    assert!(p.length > 1400.0 && p.length < 1700.0, "{}", p.length);
}

#[test]
fn path_crosses_a_seam_between_navmeshes_whose_grids_differ() {
    let mut a = grid(1, [0.0, 0.0], 4, 4, 100.0, &[], flat);
    let mut b = grid(2, [400.0, -50.0], 3, 3, 150.0, &[], flat);
    seam(&mut a, &mut b, link_kind::PORTAL);
    let g = graph(&[(a, W), (b, W)]);
    assert!(g.stats.portal_edges > 0);
    assert_eq!(g.stats.unresolved_links, 0);
    let p = g.find_path(Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: W, pos: [800.0, 300.0, 0.0] }, &PathParams::default()).unwrap();
    assert!(on_mesh(&g, W, &p.legs[0].points, 5.0));
    assert!((p.length - (750.0f32).hypot(250.0)).abs() < 2.0, "{} {:?}", p.length, p.legs[0].points);
    assert_eq!(p.legs[0].points.len(), 2, "{:?}", p.legs[0].points);
    let meshes: HashSet<u32> = p.legs[0].corridor.iter().map(|&t| g.tris[t as usize].mesh).collect();
    assert_eq!(meshes.len(), 2);
}

#[test]
fn unlinked_navmeshes_are_unreachable_and_ledges_are_opt_in() {
    let a = grid(1, [0.0, 0.0], 2, 2, 100.0, &[], flat);
    let b = grid(2, [200.0, 0.0], 2, 2, 100.0, &[], |_, _| -300.0);
    let g = graph(&[(a.clone(), W), (b.clone(), W)]);
    let from = Place { space: W, pos: [50.0, 50.0, 0.0] };
    let to = Place { space: W, pos: [350.0, 50.0, -300.0] };
    assert!(matches!(g.find_path(from, to, &PathParams::default()), Err(PathError::Unreachable { .. })));
    // The same two joined by a ledge down (and up, back).
    let (mut a, mut b) = (a, b);
    let (ea, eb) = (edges_at(&a, 200.0), edges_at(&b, 200.0));
    for &(at, ae, ..) in &ea {
        link(&mut a, at, ae, 2, eb[0].0, link_kind::LEDGE_DOWN);
    }
    for &(bt, be, ..) in &eb {
        link(&mut b, bt, be, 1, ea[0].0, link_kind::LEDGE_UP);
    }
    let g = graph(&[(a, W), (b, W)]);
    assert!(g.stats.ledge_edges > 0);
    assert!(g.find_path(from, to, &PathParams::default()).is_err());
    let ledges = PathParams { ledges: true, ..Default::default() };
    let p = g.find_path(from, to, &ledges).unwrap();
    assert_eq!(p.end().map(|e| e[2]), Some(-300.0));
    // The drop is a hop: off the top edge at x 200, onto the lower navmesh beyond it.
    let leg = &p.legs[0];
    assert_eq!(leg.hops.len(), 1, "{:?}", leg.points);
    let h = leg.hops[0];
    assert!(!h.up);
    let (off, land) = (leg.points[h.point], leg.points[h.point + 1]);
    assert!((off[0] - 200.0).abs() < 1e-3 && off[2] == 0.0, "{off:?}");
    assert!(land[0] > 210.0 && land[0] < 300.0 && land[2] == -300.0, "{land:?}");
    // Back up is 300: out of a jump's reach, so no way back.
    assert!(g.find_path(to, from, &ledges).is_err());
    assert!(g.find_path(to, from, &PathParams { ledge_rise_max: 400.0, ..ledges.clone() }).is_ok());
}

#[test]
fn walkers_take_off_from_the_ledge_links_point() {
    let a = grid(1, [0.0, 0.0], 2, 2, 100.0, &[], flat);
    let b = grid(2, [200.0, 0.0], 2, 2, 100.0, &[], |_, _| -100.0);
    let (mut a, b) = (a, b);
    let (ea, eb) = (edges_at(&a, 200.0), edges_at(&b, 200.0));
    for &(at, ae, ..) in &ea {
        link(&mut a, at, ae, 2, eb[0].0, link_kind::LEDGE_DOWN);
    }
    let g = graph(&[(a, W), (b, W)]);
    let ledges = PathParams { ledges: true, ..Default::default() };
    let to = Place { space: W, pos: [350.0, 50.0, -100.0] };
    let p = g.find_path(Place { space: W, pos: [50.0, 50.0, 0.0] }, to, &ledges).unwrap();
    let off = p.legs[0].points[p.legs[0].hops[0].point];
    // At any walking speed and frame step the flight leaves from the same point.
    for (speed, dt) in [(85.3, 1.0 / 60.0), (86.8, 1.0 / 60.0), (85.3, 1.0 / 37.0), (344.0, 1.0 / 30.0)] {
        let mut w = Walker::new(W, [50.0, 50.0, 0.0], 0.0, speed);
        w.go_to(&g, to, &ledges).unwrap();
        let mut from = None;
        for _ in 0..2000 {
            w.update(&g, dt);
            if let Some(a) = w.air {
                from = Some(a.flight.from);
                break;
            }
        }
        assert_eq!(from, Some(off), "speed {speed}, step {dt}");
    }
}

#[test]
fn low_ledges_are_jumped_up() {
    let a = grid(1, [0.0, 0.0], 2, 2, 100.0, &[], flat);
    let b = grid(2, [200.0, 0.0], 2, 2, 100.0, &[], |_, _| 40.0);
    let (mut a, mut b) = (a, b);
    let (ea, eb) = (edges_at(&a, 200.0), edges_at(&b, 200.0));
    for &(at, ae, ..) in &ea {
        link(&mut a, at, ae, 2, eb[0].0, link_kind::LEDGE_UP);
    }
    for &(bt, be, ..) in &eb {
        link(&mut b, bt, be, 1, ea[0].0, link_kind::LEDGE_DOWN);
    }
    let g = graph(&[(a, W), (b, W)]);
    let ledges = PathParams { ledges: true, ..Default::default() };
    let p = g.find_path(Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: W, pos: [350.0, 50.0, 40.0] }, &ledges).unwrap();
    assert_eq!(p.legs[0].hops.iter().map(|h| h.up).collect::<Vec<_>>(), vec![true]);
    assert_eq!(p.end().map(|e| e[2]), Some(40.0));
}

#[test]
fn doors_join_spaces_into_legs() {
    let inside = Space::Interior(0x77);
    let mut a = grid(1, [0.0, 0.0], 4, 4, 100.0, &[], flat);
    let mut b = grid(2, [5000.0, 5000.0], 3, 3, 100.0, &[], |_, _| 50.0);
    // Door 0xD1 next to a's triangle at (350, 350); door 0xD2 next to b's at (5050, 5050).
    let ta = (0..a.triangles.len()).find(|&t| {
        let v = a.triangles[t].v.map(|i| a.vertices[i as usize]);
        v.iter().all(|p| p[0] >= 300.0 && p[1] >= 300.0)
    });
    a.door_links.push(DoorLink { door: 0xD1, triangle: ta.unwrap() as u16 });
    a.triangles[ta.unwrap()].flags |= tri_flags::DOOR;
    b.door_links.push(DoorLink { door: 0xD2, triangle: 0 });
    let meshes = [(a, W), (b, inside)];
    let input: Vec<_> = meshes.iter().map(|(m, s)| (m, Some(0), Some(*s))).collect();
    let mut g = NavGraph::assemble(&input);
    g.join_doors(&input, |door| match door {
        0xD1 => Some((0xD2, [5050.0, 5030.0, 50.0], 0.5, inside)),
        0xD2 => Some((0xD1, [340.0, 340.0, 0.0], 3.0, W)),
        _ => None,
    });
    assert_eq!(g.stats.doors, 2);
    let p = g.find_path(Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: inside, pos: [5250.0, 5250.0, 50.0] }, &PathParams::default()).unwrap();
    assert_eq!(p.legs.len(), 2);
    assert_eq!(p.legs[0].space, W);
    assert_eq!(p.legs[0].door.map(|d| d.door), Some(0xD1));
    assert_eq!(p.legs[1].space, inside);
    assert_eq!(p.legs[1].points[0], [5050.0, 5030.0, 50.0]);
    // Without doors there is no way in.
    assert!(g.find_path(Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: inside, pos: [5250.0, 5250.0, 50.0] }, &PathParams { doors: false, ..Default::default() }).is_err());
    // Nor for an actor that opens no doors ("Can't Open Doors").
    assert!(g.find_path(Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: inside, pos: [5250.0, 5250.0, 50.0] }, &PathParams { opens_doors: false, ..Default::default() }).is_err());

    // A walker goes all the way through the door.
    let mut w = Walker::new(W, [50.0, 50.0, 0.0], 0.0, 100.0);
    w.go_to(&g, Place { space: inside, pos: [5250.0, 5250.0, 50.0] }, &PathParams::default()).unwrap();
    let mut events = Vec::new();
    for _ in 0..2000 {
        match w.update(&g, 0.05) {
            WalkEvent::Moved | WalkEvent::Turned => {}
            e => events.push(e),
        }
        if w.follower.is_none() {
            break;
        }
    }
    assert!(matches!(events[0], WalkEvent::UsedDoor(_)), "{events:?}");
    assert_eq!(events.last(), Some(&WalkEvent::Arrived));
    assert_eq!(w.space, inside);
    assert!((w.feet[0] - 5250.0).abs() < 1.0 && (w.feet[1] - 5250.0).abs() < 1.0);
}

#[test]
fn a_walker_stops_and_faces_a_load_door_before_going_through() {
    use crate::follow::{heading_delta, heading_of};
    let inside = Space::Interior(0x77);
    let mut a = grid(1, [0.0, 0.0], 4, 4, 100.0, &[], flat);
    let b = grid(2, [5000.0, 5000.0], 3, 3, 100.0, &[], |_, _| 50.0);
    let ta = (0..a.triangles.len())
        .find(|&t| a.triangles[t].v.map(|i| a.vertices[i as usize]).iter().all(|p| p[0] >= 300.0 && p[1] >= 300.0))
        .unwrap();
    a.door_links.push(DoorLink { door: 0xD1, triangle: ta as u16 });
    a.triangles[ta].flags |= tri_flags::DOOR;
    let meshes = [(a, W), (b, inside)];
    let input: Vec<_> = meshes.iter().map(|(m, s)| (m, Some(0), Some(*s))).collect();
    let mut g = NavGraph::assemble(&input);
    g.join_doors(&input, |door| (door == 0xD1).then_some((0xD2, [5050.0, 5030.0, 50.0], 0.5, inside)));
    // The door stands due north of where the path ends; the walk comes up from the
    // south-west, at about 45 degrees to it.
    let door = [360.0, 700.0, 0.0];
    g.place_doors(|d| (d == 0xD1).then_some(door));

    let mut w = Walker::new(W, [50.0, 50.0, 0.0], 0.0, 100.0);
    w.go_to(&g, Place { space: inside, pos: [5250.0, 5250.0, 50.0] }, &PathParams::default()).unwrap();
    let (mut turns_at_door, mut stood_at) = (0, None);
    for _ in 0..2000 {
        let (feet, heading) = (w.feet, w.heading);
        match w.update(&g, 0.05) {
            WalkEvent::Moved => turns_at_door = 0,
            WalkEvent::Turned => {
                // Turning on the spot: the feet stay put.
                assert_eq!(w.feet, feet);
                turns_at_door += 1;
                stood_at = Some(feet);
            }
            WalkEvent::UsedDoor(d) => {
                assert_eq!(d.door_at, Some(door));
                assert!(turns_at_door > 0, "went through without stopping to turn");
                assert_eq!(stood_at, Some(feet));
                let want = heading_of([door[0] - feet[0], door[1] - feet[1]]);
                assert!(heading_delta(heading, want).abs() < 1e-3, "faced {heading}, door at {want}");
                assert!(w.door_off.is_some_and(|o| o < 1e-3));
                assert_eq!(w.space, inside);
                return;
            }
            e => panic!("unexpected {e:?}"),
        }
    }
    panic!("never reached the door");
}

#[test]
fn routes_chain_interiors_and_worldspaces_and_respect_locks() {
    use crate::graph::DoorLock;
    // Worldspace W -> interior A -> interior B -> worldspace V, each its own navmesh far
    // apart; a door near each mesh's north-east corner leads on to the next one's
    // south-west corner. B's door on to V is locked with key 0x4B.
    let (a, b, v) = (Space::Interior(0xA), Space::Interior(0xB), Space::World(0x56));
    let spaces = [W, a, b, v];
    let mut meshes: Vec<NavMeshRecord> = (0..4).map(|i| grid(10 + i as u32, [i as f32 * 10_000.0, 0.0], 4, 4, 100.0, &[], flat)).collect();
    let corner = |m: &NavMeshRecord| {
        let x0 = m.vertices[0][0];
        (0..m.triangles.len()).find(|&t| m.triangles[t].v.iter().all(|&i| m.vertices[i as usize][0] >= x0 + 300.0 && m.vertices[i as usize][1] >= 300.0)).unwrap()
    };
    for (i, m) in meshes.iter_mut().enumerate().take(3) {
        let t = corner(m);
        m.door_links.push(DoorLink { door: 0xD0 + i as u32, triangle: t as u16 });
    }
    let input: Vec<_> = meshes.iter().zip(spaces).map(|(m, s)| (m, Some(0), Some(s))).collect();
    let mut g = NavGraph::assemble(&input);
    g.join_doors(&input, |door| {
        let i = (door - 0xD0) as usize;
        let next = (i + 1) as f32 * 10_000.0;
        Some((0xE0 + i as u32, [next + 50.0, 50.0, 0.0], 0.0, spaces[i + 1]))
    });
    g.set_door_lock(0xD2, Some(DoorLock { level: 255, key: Some(0x4B) }));
    assert_eq!(g.stats.doors, 3);
    let from = Place { space: W, pos: [50.0, 50.0, 0.0] };
    let to = Place { space: v, pos: [30_350.0, 350.0, 0.0] };
    let open = PathParams::default();
    // Locked without the key: no way through.
    assert!(!g.can_reach(from, to, &open));
    assert!(matches!(g.find_path(from, to, &open), Err(PathError::Unreachable { .. })));
    // An interior short of the lock is still reachable, through one door.
    let inside_a = Place { space: a, pos: [10_200.0, 200.0, 0.0] };
    assert_eq!(g.door_route(from, inside_a, &open).unwrap().len(), 1);
    // With the key: three doors, four legs, ending in the other worldspace.
    let keyed = PathParams { keys: vec![0x4B], ..Default::default() };
    let route = g.door_route(from, to, &keyed).unwrap();
    assert_eq!(route.iter().map(|d| d.door).collect::<Vec<_>>(), vec![0xD0, 0xD1, 0xD2]);
    let p = g.find_path(from, to, &keyed).unwrap();
    assert_eq!(p.legs.iter().map(|l| l.space).collect::<Vec<_>>(), spaces.to_vec());
    assert_eq!(p.end(), Some([30_350.0, 350.0, 0.0]));
    // Unlocking the door at run time opens the way for anyone.
    g.set_door_lock(0xD2, None);
    assert!(g.can_reach(from, to, &open));
    // The game's lock states brought in: 0xD1 locked by a script; a door the graph
    // does not know is passed over; 0xD0 and 0xD2, not named, keep theirs.
    assert_eq!(g.door_refs(), vec![0xD0, 0xD1, 0xD2]);
    let lock = DoorLock { level: 50, key: None };
    let changes = g.sync_door_locks([(0xD1, Some(lock)), (0xFF, Some(lock))]);
    assert_eq!(changes, crate::graph::DoorLockChanges { unlocked: vec![], locked: vec![0xD1] });
    assert!(!g.door_locks.contains_key(&0xFF));
    assert!(!g.can_reach(from, to, &open));
    assert!(g.sync_door_locks([(0xD1, Some(lock))]).is_empty(), "nothing new");
    // Unlocked again (a script's Unlock): the way is open and the door is reported.
    let changes = g.sync_door_locks([(0xD0, None), (0xD1, None)]);
    assert_eq!(changes.unlocked, vec![0xD1]);
    assert!(g.can_reach(from, to, &open));
}

#[test]
fn walker_follows_the_surface_on_a_slope() {
    let slope = |x: f32, _y: f32| x * 0.25;
    let g = graph(&[(grid(1, [0.0, 0.0], 8, 8, 100.0, &[(3, 3), (3, 4), (4, 3), (4, 4)], slope), W)]);
    let mut w = Walker::new(W, [50.0, 50.0, 12.5], 0.0, 150.0);
    w.go_to(&g, Place { space: W, pos: [750.0, 750.0, 187.5] }, &PathParams::default()).unwrap();
    let mut steps = 0;
    while w.follower.is_some() && steps < 10_000 {
        w.update(&g, 1.0 / 60.0);
        steps += 1;
        assert!((w.feet[2] - slope(w.feet[0], w.feet[1])).abs() < 0.5, "off the surface at {:?}", w.feet);
        assert!(w.tri == NONE || g.contains_xy(w.tri, w.feet[0], w.feet[1]));
    }
    assert!(w.follower.is_none(), "did not arrive");
    assert!((w.feet[0] - 750.0).abs() < 1.0 && (w.feet[1] - 750.0).abs() < 1.0, "{:?}", w.feet);
    // It went round the hole in the middle, so further than the straight line.
    assert!(w.travelled > 700.0 * std::f32::consts::SQRT_2);
}

#[test]
fn disabled_navmesh_blocks_paths() {
    let mut a = grid(1, [0.0, 0.0], 2, 2, 100.0, &[], flat);
    let mut b = grid(2, [200.0, 0.0], 2, 2, 100.0, &[], flat);
    seam(&mut a, &mut b, link_kind::PORTAL);
    let mut g = graph(&[(a, W), (b, W)]);
    let (from, to) = (Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: W, pos: [350.0, 150.0, 0.0] });
    assert_eq!(g.find_path(from, to, &PathParams::default()).unwrap().goal_snap, 0.0);
    assert!(g.set_enabled(2, false));
    // The goal is no longer on a usable triangle: it snaps 150 units onto the first
    // navmesh's edge, and the path says so.
    assert_eq!(g.locate(W, to.pos, 10.0), None);
    let p = g.find_path(from, to, &PathParams::default()).unwrap();
    assert!((p.goal_snap - 150.0).abs() < 0.5, "{}", p.goal_snap);
    assert!(!g.set_enabled(9, false));
}

/// The scripts' switches (`EnableNavMesh` / `DisableNavmesh`) applied to the graph: a
/// path planned over a navmesh disabled since is blocked, and the way across is gone
/// until it is enabled again; a navmesh the load order has disabled is enabled by its
/// switch and falls back to disabled without one.
#[test]
fn navmesh_switches_block_and_reopen_the_way() {
    let mut a = grid(1, [0.0, 0.0], 2, 2, 100.0, &[], flat);
    let mut b = grid(2, [200.0, 0.0], 2, 2, 100.0, &[], flat);
    let mut c = grid(3, [400.0, 0.0], 2, 2, 100.0, &[], flat);
    seam(&mut a, &mut b, link_kind::PORTAL);
    seam(&mut b, &mut c, link_kind::PORTAL);
    let mut g = graph(&[(a, W), (b, W), (c, W)]);
    let (from, to) = (Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: W, pos: [550.0, 150.0, 0.0] });
    let mut w = Walker::new(W, from.pos, 0.0, 100.0);
    w.go_to(&g, to, &PathParams::default()).unwrap();
    assert!(!w.follower.as_ref().unwrap().blocked(&g));
    let mut switches = std::collections::BTreeMap::new();
    switches.insert(2, false);
    assert!(g.apply_switches(&switches));
    assert!(!g.apply_switches(&switches), "applied once");
    assert!(w.follower.as_ref().unwrap().blocked(&g));
    assert!(!g.can_reach(from, to, &PathParams::default()));
    switches.insert(2, true);
    assert!(g.apply_switches(&switches));
    assert!(!w.follower.as_ref().unwrap().blocked(&g));
    assert!(g.can_reach(from, to, &PathParams::default()));
    // Initially disabled (as the record's flag would make it): enabled only by a switch.
    // (The goal then lies on no usable triangle: a path would snap it onto the middle
    // navmesh's edge.)
    g.meshes[2].initially_enabled = false;
    assert!(g.apply_switches(&std::collections::BTreeMap::new()));
    assert_eq!(g.locate(W, to.pos, 10.0), None);
    switches.insert(3, true);
    assert!(g.apply_switches(&switches));
    assert!(g.locate(W, to.pos, 10.0).is_some());
}

/// A door that opens in place across a corridor: 100 wide (its model's x), 8 thick
/// (y), turned so it spans the corridor (game y) at `x`, from y = `y0` to `y0 + 100`.
fn hinged(door: u32, x: f32, y0: f32) -> crate::doors::InPlaceDoor {
    crate::doors::InPlaceDoor {
        door,
        space: W,
        position: [x, y0 + 50.0, 0.0],
        // Model x along game y, model y along game -x.
        rotation: [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
        bounds: ([-50.0, -4.0, 0.0], [50.0, 4.0, 200.0]),
    }
}

/// A corridor 1,000 long and 100 wide with a door across it at x = 500.
fn corridor_with_door(lock: Option<crate::graph::DoorLock>) -> NavGraph {
    let mut g = graph(&[(grid(1, [0.0, 0.0], 10, 1, 100.0, &[], flat), W)]);
    g.set_in_place_doors(vec![hinged(0xD00, 500.0, 0.0)], |_| lock);
    g
}

#[test]
fn a_path_through_a_door_that_opens_in_place_stops_in_front_of_it() {
    let g = corridor_with_door(None);
    assert_eq!(g.stats.in_place_doors, 1);
    assert!(!g.door_tris.is_empty() && g.door_tris.values().all(|d| d == &vec![0xD00]));
    let from = Place { space: W, pos: [50.0, 50.0, 0.0] };
    let to = Place { space: W, pos: [950.0, 50.0, 0.0] };
    let p = g.find_path(from, to, &PathParams::default()).unwrap();
    assert_eq!(p.legs.len(), 1);
    let leg = &p.legs[0];
    assert_eq!(leg.door_stops.len(), 1);
    let s = leg.door_stops[0];
    assert_eq!(s.door, 0xD00);
    // The clearance (24) and the opening's width (the navmesh along the door's line,
    // 96-100 at the 4-unit sampling step: as far as a leaf can swing) before the door at
    // x = 496, in line with the middle of the opening.
    let at = leg.points[s.point];
    assert!((at[0] - (496.0 - 122.0)).abs() <= 2.5 && (at[1] - 50.0).abs() < 0.5, "{at:?}");
    // The polyline is otherwise unchanged: straight along the corridor.
    assert!(leg.points.iter().all(|q| (q[1] - 50.0).abs() < 0.5));

    // The follower walks to the stop, then holds there until the door has opened.
    let mut f = crate::Follower::new(p.clone());
    match f.steer(from.pos) {
        crate::Steer::Walk { target, .. } => assert_eq!(target, at),
        s => panic!("{s:?}"),
    }
    let near = [at[0] - 10.0, 50.0, 0.0];
    assert_eq!(f.steer(near), crate::Steer::OpenDoor { door: 0xD00, at });
    // Still held a moment later, even pushed away from the stop by the opening door.
    assert_eq!(f.steer(near), crate::Steer::OpenDoor { door: 0xD00, at });
    assert_eq!(f.steer([at[0] - 60.0, 70.0, 0.0]), crate::Steer::OpenDoor { door: 0xD00, at });
    f.door_opened();
    // Straight through the middle of the opening to a clearance (24) and the margin (24)
    // beyond the door's far face (x = 504), then on.
    match f.steer(near) {
        crate::Steer::Walk { target, .. } => assert_eq!(target, [552.0, 50.0, 0.0]),
        s => panic!("{s:?}"),
    }
    match f.steer([552.0, 50.0, 0.0]) {
        crate::Steer::Walk { target, .. } => assert_eq!(target, [950.0, 50.0, 0.0]),
        s => panic!("{s:?}"),
    }
    assert_eq!(f.steer([950.0, 50.0, 0.0]), crate::Steer::Arrived);

    // An actor that overshoots the stop is held as well.
    let mut f = crate::Follower::new(p);
    assert!(matches!(f.steer(from.pos), crate::Steer::Walk { .. }));
    assert!(matches!(f.steer([at[0] + 20.0, 50.0, 0.0]), crate::Steer::OpenDoor { door: 0xD00, .. }));
}

#[test]
fn walker_waits_for_the_door_to_open() {
    let g = corridor_with_door(None);
    let mut w = Walker::new(W, [50.0, 50.0, 0.0], 0.0, 100.0);
    w.go_to(&g, Place { space: W, pos: [950.0, 50.0, 0.0] }, &PathParams::default()).unwrap();
    // A door that takes 1.67 s to open, started by the first request.
    let mut opened_at: Option<f32> = None;
    let mut time = 0.0f32;
    let dt = 0.1;
    let mut waiting = Vec::new();
    let mut asked = Vec::new();
    for _ in 0..400 {
        time += dt;
        let ev = w.update_with(&g, dt, &mut |d| {
            asked.push(d);
            let t0 = *opened_at.get_or_insert(time);
            time - t0 >= 1.67
        });
        if let WalkEvent::WaitingForDoor(d) = ev {
            waiting.push((time, w.feet));
            assert_eq!(d, 0xD00);
        }
        if ev == WalkEvent::Arrived {
            break;
        }
    }
    assert!(w.follower.is_none(), "arrived");
    assert!((w.feet[0] - 950.0).abs() < 1.0);
    assert!(asked.iter().all(|&d| d == 0xD00));
    // It stood still in front of the door while it opened, then walked on.
    let (first, last) = (waiting.first().unwrap(), waiting.last().unwrap());
    assert!(last.0 - first.0 >= 1.5, "waited {:.2} s", last.0 - first.0);
    assert_eq!(first.1, last.1);
    assert!((first.1[0] - 372.0).abs() < 17.0, "{:?}", first.1);
    // A door already open is not waited for.
    let mut w = Walker::new(W, [50.0, 50.0, 0.0], 0.0, 100.0);
    w.go_to(&g, Place { space: W, pos: [950.0, 50.0, 0.0] }, &PathParams::default()).unwrap();
    let mut events = Vec::new();
    for _ in 0..200 {
        let ev = w.update_with(&g, dt, &mut |_| true);
        events.push(ev);
        if ev == WalkEvent::Arrived {
            break;
        }
    }
    assert!(events.contains(&WalkEvent::Arrived));
    assert!(!events.iter().any(|e| matches!(e, WalkEvent::WaitingForDoor(_))));
}

#[test]
fn locked_doors_that_open_in_place_close_the_way_without_the_key() {
    use crate::graph::DoorLock;
    let mut g = corridor_with_door(Some(DoorLock { level: 50, key: Some(0x4B) }));
    let from = Place { space: W, pos: [50.0, 50.0, 0.0] };
    let to = Place { space: W, pos: [950.0, 50.0, 0.0] };
    assert!(matches!(g.find_path(from, to, &PathParams::default()), Err(PathError::Unreachable { .. })));
    // Short of the door is still reachable.
    assert!(g.find_path(from, Place { space: W, pos: [350.0, 50.0, 0.0] }, &PathParams::default()).is_ok());
    // Its owner may use it without the key (Fallout 3's playroom door, Dad's).
    for p in [PathParams { keys: vec![0x4B], ..Default::default() }, PathParams { open_locked: true, ..Default::default() }, PathParams { owned_doors: vec![0xD00], ..Default::default() }] {
        let path = g.find_path(from, to, &p).unwrap();
        assert_eq!(path.legs[0].door_stops.len(), 1);
    }
    // Unlocked at run time.
    g.set_door_lock(0xD00, None);
    assert!(g.find_path(from, to, &PathParams::default()).is_ok());
}

#[test]
fn a_path_beside_a_door_does_not_stop() {
    // Three lanes; the door closes only the southern one, at x = 500.
    let mut g = graph(&[(grid(1, [0.0, 0.0], 10, 3, 100.0, &[], flat), W)]);
    g.set_in_place_doors(vec![hinged(0xD00, 500.0, 0.0)], |_| None);
    let north = |x: f32| Place { space: W, pos: [x, 250.0, 0.0] };
    let south = |x: f32| Place { space: W, pos: [x, 50.0, 0.0] };
    let p = g.find_path(north(50.0), north(950.0), &PathParams::default()).unwrap();
    assert!(p.legs[0].door_stops.is_empty());
    // Through it, it stops.
    let p = g.find_path(south(50.0), south(950.0), &PathParams::default()).unwrap();
    assert_eq!(p.legs[0].door_stops.len(), 1);
    // Locked, the path goes round it through the open lanes.
    g.set_door_lock(0xD00, Some(crate::graph::DoorLock { level: 1, key: None }));
    let p = g.find_path(south(50.0), south(950.0), &PathParams::default()).unwrap();
    assert!(p.legs[0].door_stops.is_empty());
    assert!(p.legs[0].points.iter().any(|q| q[1] > 100.0), "went round: {:?}", p.legs[0].points);
}

#[test]
fn an_actor_starting_at_the_door_backs_off_to_open_it() {
    let g = corridor_with_door(None);
    // Starting 20 units in front of the door: it steps back to the stop, clear of the
    // leaf, before opening it.
    let from = Place { space: W, pos: [476.0, 50.0, 0.0] };
    let p = g.find_path(from, Place { space: W, pos: [950.0, 50.0, 0.0] }, &PathParams::default()).unwrap();
    let leg = &p.legs[0];
    let stop = leg.points[leg.door_stops[0].point];
    assert!((stop[0] - 374.0).abs() <= 2.5, "{stop:?}");
    let mut f = crate::Follower::new(p);
    match f.steer(from.pos) {
        crate::Steer::Walk { target, .. } => assert_eq!(target, stop),
        s => panic!("{s:?}"),
    }
    assert!(matches!(f.steer([380.0, 50.0, 0.0]), crate::Steer::OpenDoor { door: 0xD00, .. }));
    f.door_opened();
    assert!(matches!(f.steer([380.0, 50.0, 0.0]), crate::Steer::Walk { .. }));
}

#[test]
fn a_path_planned_again_within_the_approach_to_an_open_door_goes_straight_on() {
    let g = corridor_with_door(None);
    // 76 units before the door, past the stop at 374 (a follower whose path is planned
    // again on its way through).
    let from = Place { space: W, pos: [420.0, 50.0, 0.0] };
    let to = Place { space: W, pos: [950.0, 50.0, 0.0] };
    // Closed (or not known to be open): back to the stop, clear of the leaf.
    let p = g.find_path(from, to, &PathParams::default()).unwrap();
    let leg = &p.legs[0];
    assert!((leg.points[leg.door_stops[0].point][0] - 374.0).abs() <= 2.5);
    // Open: the stop is where it is, and on through the opening.
    let p = g.find_path(from, to, &PathParams { open_doors: vec![0xD00], ..PathParams::default() }).unwrap();
    let leg = &p.legs[0];
    assert_eq!(leg.door_stops[0].point, 0);
    assert!(leg.points.iter().all(|q| q[0] >= 420.0 - 0.5), "{:?}", leg.points);
    let mut f = crate::Follower::new(p);
    assert!(matches!(f.steer(from.pos), crate::Steer::OpenDoor { door: 0xD00, .. }));
    f.door_opened();
    match f.steer(from.pos) {
        crate::Steer::Walk { target, .. } => assert_eq!(target, [552.0, 50.0, 0.0]),
        s => panic!("{s:?}"),
    }
}

#[test]
fn a_diagonal_approach_is_squared_up_to_the_middle_of_the_door() {
    // A room 500 x 400 south of a wall at y = 400 with a doorway 100 wide in it
    // (x 200..300), a room north of it; the door (100 wide, hinged at an end) closes
    // the doorway. The path comes from the room's far corner, diagonally.
    let holes: Vec<(usize, usize)> = (0..10).filter(|&i| !(4..6).contains(&i)).map(|i| (i, 8)).collect();
    let mut g = graph(&[(grid(1, [0.0, 0.0], 10, 17, 50.0, &holes, flat), W)]);
    let door = crate::doors::InPlaceDoor {
        door: 0xD01,
        space: W,
        position: [200.0, 425.0, 0.0],
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        scale: 1.0,
        bounds: ([0.0, -4.0, 0.0], [100.0, 4.0, 200.0]),
    };
    g.set_in_place_doors(vec![door], |_| None);
    let p = g.find_path(Place { space: W, pos: [480.0, 20.0, 0.0] }, Place { space: W, pos: [20.0, 830.0, 0.0] }, &PathParams::default()).unwrap();
    let leg = &p.legs[0];
    assert_eq!(leg.door_stops.len(), 1);
    let i = leg.door_stops[0].point;
    let (stop, through) = (leg.points[i], leg.points[i + 1]);
    // In line with the middle of the doorway (x = 250), in front of it and beyond it.
    assert!((stop[0] - 250.0).abs() < 1.0 && stop[1] < 425.0 - 100.0, "{stop:?}");
    assert!((through[0] - 250.0).abs() < 1.0 && through[1] > 429.0, "{through:?}");
    assert!(on_mesh(&g, W, &leg.points, 5.0));
}

#[test]
fn cover_edges_are_the_outline_of_listed_triangles_facing_out() {
    // A 4 × 4 grid of 100-unit squares with a hole (a rock) at square (2, 2); the four
    // triangles round the hole are listed as cover.
    let mut m = grid(0x10, [0.0, 0.0], 4, 4, 100.0, &[(2, 2)], flat);
    let listed: Vec<u16> = (0..m.triangles.len() as u16)
        .filter(|&t| {
            let c = m.triangles[t as usize].v.map(|v| m.vertices[v as usize]);
            let (x, y) = ((c[0][0] + c[1][0] + c[2][0]) / 3.0, (c[0][1] + c[1][1] + c[2][1]) / 3.0);
            (150.0..=350.0).contains(&x) && (150.0..=350.0).contains(&y) && !((200.0..300.0).contains(&x) && (200.0..300.0).contains(&y))
        })
        .collect();
    m.cover_triangles = listed;
    let g = graph(&[(m, W)]);
    let edges = g.cover_edges_near(W, [250.0, 100.0, 0.0], 400.0);
    // Only the hole's four sides lie inside listed triangles (the grid's outer edges are
    // not listed).
    assert_eq!(edges.len(), 4, "{edges:?}");
    // The south side of the rock: its middle at (250, 200), out of the navmesh is north.
    let south = edges[0];
    assert!((south.mid[0] - 250.0).abs() < 1e-3 && (south.mid[1] - 200.0).abs() < 1e-3);
    assert!((south.out[1] - 1.0).abs() < 1e-3 && south.out[0].abs() < 1e-3);
    // A threat north of the rock is covered from there; one south of it is not.
    assert!(south.faces([250.0, 380.0, 0.0]) && !south.faces([250.0, 20.0, 0.0]));
    assert_eq!(south.behind(40.0), [250.0, 160.0, 0.0]);
    assert!((south.length() - 100.0).abs() < 1e-3);
}

/// A floor, a ramp up from its east side to a landing, and a deck turning back west
/// from the landing over the floor, 200 units above it (a catwalk over a factory floor:
/// RocketLabMid's flights and catwalks, docs/spec-notes/navmesh.md §4b).
fn floor_with_deck_above() -> NavGraph {
    let mut floor = grid(1, [0.0, 0.0], 4, 4, 100.0, &[], flat);
    let mut ramp = grid(2, [400.0, 0.0], 4, 1, 100.0, &[], |x, _| (x - 400.0) * 0.5);
    let mut landing = grid(3, [800.0, 0.0], 1, 4, 100.0, &[], |_, _| 200.0);
    let mut deck = grid(4, [0.0, 300.0], 8, 1, 100.0, &[], |_, _| 200.0);
    seam(&mut floor, &mut ramp, link_kind::PORTAL);
    seam(&mut ramp, &mut landing, link_kind::PORTAL);
    seam(&mut deck, &mut landing, link_kind::PORTAL);
    graph(&[(floor, W), (ramp, W), (landing, W), (deck, W)])
}

#[test]
fn a_straight_line_does_not_reach_a_level_stacked_over_the_floor() {
    let g = floor_with_deck_above();
    let p = PathParams::default();
    let (low, high) = ([150.0, 350.0, 0.0], [150.0, 350.0, 200.0]);
    let floor = g.locate(W, low, 10.0).expect("floor triangle");
    let deck = g.locate(W, high, 10.0).expect("deck triangle");
    assert_ne!(floor, deck);
    // Seen from above the deck's spot is on the floor triangle; it is not walkable from
    // there all the same.
    assert_eq!(g.walkable_line(W, floor, low, high, 18.0, &p), None);
    assert_eq!(g.walkable_line(W, floor, [50.0, 50.0, 0.0], high, 18.0, &p), None);
    // On one level the line is walkable.
    assert_eq!(g.walkable_line(W, floor, [50.0, 50.0, 0.0], low, 18.0, &p), Some(floor));
    assert_eq!(g.walkable_line(W, deck, [650.0, 350.0, 200.0], high, 18.0, &p), Some(deck));
}

#[test]
fn a_path_up_to_a_level_stacked_over_the_floor_takes_the_ramp() {
    // RocketLabMid (owner report 2026-10-23): the path from the floor to the top
    // catwalk was pulled straight from the floor under the catwalk up to it.
    let g = floor_with_deck_above();
    for (from, to) in [([150.0, 350.0, 0.0], [150.0, 350.0, 200.0]), ([150.0, 350.0, 200.0], [150.0, 350.0, 0.0]), ([50.0, 50.0, 0.0], [100.0, 350.0, 200.0])] {
        let path = g.find_path(Place { space: W, pos: from }, Place { space: W, pos: to }, &PathParams::default()).unwrap();
        let pts = &path.legs[0].points;
        // Up the ramp (x 400-800) and back along the deck: well over 1000 units.
        assert!(path.length > 1000.0, "{pts:?}");
        assert!(pts.iter().any(|q| q[0] > 700.0), "{pts:?}");
        // No segment rises or falls more steeply than the ramp (1 in 2).
        for w in pts.windows(2) {
            let h = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
            assert!((w[1][2] - w[0][2]).abs() <= 0.5 * h + 1.0, "{w:?} in {pts:?}");
        }
    }
}

/// A broken mod's navmesh with triangles but no vertices (last in the graph, so its
/// triangles' fallback index is past the end) is assembled with them left out.
#[test]
fn a_navmesh_with_triangles_but_no_vertices_is_skipped() {
    let good = grid(0x10, [0.0, 0.0], 2, 2, 100.0, &[], flat);
    let bad = NavMeshRecord {
        form_id: 0x11,
        triangles: vec![Triangle { v: [0, 1, 2], edges: [0, -1, 5], flags: 0, cover: 0 }; 2],
        ..Default::default()
    };
    let g = graph(&[(good, W), (bad, W)]);
    let first_bad = g.meshes[1].first_tri;
    assert!(g.tris[first_bad as usize].has(tri_flags::DELETED));
    assert!(!g.usable(first_bad));
    assert_eq!(g.corners(first_bad), [[0.0; 3]; 3]);
}

/// Sandbags across the corridor at x = 500 (as [`hinged`]'s door: 100 wide, 8 thick),
/// `height` tall from z = `base`, initially disabled.
fn sandbags(base: f32, height: f32) -> crate::obstacles::Obstacle {
    let d = hinged(0x5B0, 500.0, 0.0);
    crate::obstacles::Obstacle { reference: 0x5B0, space: W, position: [d.position[0], d.position[1], base], rotation: d.rotation, scale: 1.0, bounds: ([-50.0, -4.0, 0.0], [50.0, 4.0, height]), initially_enabled: false }
}

/// An obstacle that comes and goes (ESM full audit gap 22, `crate::obstacles`): while
/// enabled it cuts the triangles under it and the corridor is shut; a path planned
/// across it before is blocked; disabled again, the way is open.
#[test]
fn an_enabled_obstacle_cuts_the_navmesh_under_it() {
    let mut g = graph(&[(grid(1, [0.0, 0.0], 10, 1, 100.0, &[], flat), W)]);
    g.set_obstacles(vec![sandbags(0.0, 90.0)], |o| o.initially_enabled);
    assert_eq!(g.obstacles.len(), 1);
    assert_eq!(g.cut_triangles(), 0, "disabled: nothing cut");
    let (from, to) = (Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: W, pos: [950.0, 50.0, 0.0] });
    assert!(g.can_reach(from, to, &PathParams::default()));
    let mut w = Walker::new(W, from.pos, 0.0, 100.0);
    w.go_to(&g, to, &PathParams::default()).unwrap();
    assert_eq!(g.sync_obstacles([(0x5B0, true), (0x999, true)]), vec![0x5B0]);
    assert!(g.sync_obstacles([(0x5B0, true)]).is_empty(), "applied once");
    // The triangles touching x = 496..504: the squares on both sides of the line.
    assert!(g.cut_triangles() >= 2);
    assert!(w.follower.as_ref().unwrap().blocked(&g));
    assert!(!g.can_reach(from, to, &PathParams::default()));
    assert_eq!(g.sync_obstacles([(0x5B0, false)]), vec![0x5B0]);
    assert_eq!(g.cut_triangles(), 0);
    assert!(g.can_reach(from, to, &PathParams::default()));
}

/// The obstacle stamp changes with every change of the obstacles or their states and
/// only then; no two graphs share one once they learnt obstacles (`fnv_ai`'s obstacle
/// poll is keyed on it).
#[test]
fn the_obstacle_stamp_changes_with_the_obstacles_only() {
    let mut g = graph(&[(grid(1, [0.0, 0.0], 10, 1, 100.0, &[], flat), W)]);
    assert_eq!(g.obstacles_stamp(), 0, "none learnt");
    g.set_obstacles(vec![sandbags(0.0, 90.0)], |o| o.initially_enabled);
    let learnt = g.obstacles_stamp();
    assert_ne!(learnt, 0);
    assert!(g.sync_obstacles([(0x5B0, false), (0x999, true)]).is_empty());
    assert_eq!(g.obstacles_stamp(), learnt, "nothing changed");
    assert_eq!(g.sync_obstacles([(0x5B0, true)]), vec![0x5B0]);
    let switched = g.obstacles_stamp();
    assert_ne!(switched, learnt);
    assert_eq!(g.sync_obstacles([(0x5B0, false)]), vec![0x5B0]);
    assert!(g.obstacles_stamp() != switched && g.obstacles_stamp() != learnt, "back to the first states, still a new stamp");
    let mut other = graph(&[(grid(1, [0.0, 0.0], 10, 1, 100.0, &[], flat), W)]);
    other.set_obstacles(vec![sandbags(0.0, 90.0)], |o| o.initially_enabled);
    assert_ne!(other.obstacles_stamp(), g.obstacles_stamp());
    g.set_obstacles(vec![sandbags(0.0, 90.0)], |o| o.initially_enabled);
    assert_ne!(g.obstacles_stamp(), other.obstacles_stamp(), "learnt again");
}

/// An obstacle placed enabled (with an enable parent, not Initially Disabled) stood there
/// when the navmesh was made: it never cuts, enabled or not (the Hoover Dam sandbag on
/// Colonel Moore's way to Kimball's podium, `crate::obstacles`).
#[test]
fn an_obstacle_placed_enabled_does_not_cut() {
    let mut g = graph(&[(grid(1, [0.0, 0.0], 10, 1, 100.0, &[], flat), W)]);
    let placed = crate::obstacles::Obstacle { initially_enabled: true, ..sandbags(0.0, 90.0) };
    g.set_obstacles(vec![placed], |o| o.initially_enabled);
    assert_eq!(g.obstacles.len(), 1, "still learnt");
    assert_eq!(g.cut_triangles(), 0);
    let (from, to) = (Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: W, pos: [950.0, 50.0, 0.0] });
    assert!(g.can_reach(from, to, &PathParams::default()));
    assert!(g.sync_obstacles([(0x5B0, false)]).len() == 1 && g.cut_triangles() == 0);
    assert!(g.sync_obstacles([(0x5B0, true)]).len() == 1 && g.cut_triangles() == 0);
    assert!(g.can_reach(from, to, &PathParams::default()));
}

/// The floor near an obstacle's top is on it, not under it, and a floor far below its
/// base is another level: neither is cut.
#[test]
fn an_obstacle_cuts_only_floor_within_its_height() {
    let floor = [[490.0, 0.0, 0.0], [510.0, 0.0, 0.0], [500.0, 100.0, 0.0]];
    assert!(sandbags(0.0, 90.0).cuts(floor));
    // Sunk until its top is 10 above the floor.
    assert!(!sandbags(-80.0, 90.0).cuts(floor));
    // Standing on a deck 200 above the floor.
    assert!(!sandbags(200.0, 90.0).cuts(floor));
    // Beside the floor, seen from above.
    assert!(!sandbags(0.0, 90.0).cuts([[600.0, 0.0, 0.0], [700.0, 0.0, 0.0], [650.0, 100.0, 0.0]]));
}

#[test]
fn places_to_avoid_cost_more_and_are_gone_round() {
    let g = graph(&[(grid(1, [0.0, 0.0], 10, 10, 100.0, &[], flat), W)]);
    let (from, to) = (Place { space: W, pos: [150.0, 150.0, 0.0] }, Place { space: W, pos: [850.0, 150.0, 0.0] });
    // Radiated ground round (500, 150), radius 220: straight through without the cost,
    // round its north edge with it.
    let straight = g.find_path(from, to, &PathParams::default()).unwrap();
    assert!(straight.length < 720.0, "{}", straight.length);
    let p = PathParams { avoid: vec![([500.0, 150.0, 0.0], 220.0)], avoid_cost: 8.0, ..Default::default() };
    let round = g.find_path(from, to, &p).unwrap();
    assert!(round.length > 760.0, "{}", round.length);
    assert!(round.legs[0].points.iter().any(|q| q[1] > 300.0), "{:?}", round.legs[0].points);
}

#[test]
fn water_triangles_close_to_actors_that_do_not_swim() {
    // A strip of water across x = 400..500 from y = 0 to 800; land round it at the north.
    let mut m = grid(1, [0.0, 0.0], 10, 10, 100.0, &[], flat);
    for i in 0..m.triangles.len() {
        let t = &m.triangles[i];
        let c = t.v.iter().fold([0.0f32; 2], |c, &v| [c[0] + m.vertices[v as usize][0] / 3.0, c[1] + m.vertices[v as usize][1] / 3.0]);
        if (400.0..500.0).contains(&c[0]) && c[1] < 800.0 {
            m.triangles[i].flags |= tri_flags::WATER;
        }
    }
    let g = graph(&[(m, W)]);
    let (from, to) = (Place { space: W, pos: [150.0, 150.0, 0.0] }, Place { space: W, pos: [850.0, 150.0, 0.0] });
    let wet = |p: &crate::path::Path| p.legs[0].corridor.iter().filter(|&&t| g.tris[t as usize].has(tri_flags::WATER)).count();
    // A swimmer crosses the strip (three times the cost of land, still shorter).
    let swim = g.find_path(from, to, &PathParams::default()).unwrap();
    assert!(wet(&swim) > 0 && swim.length < 900.0, "{} {:?}", swim.length, swim.legs[0].points);
    // One that does not swim goes round it by land.
    let dry = g.find_path(from, to, &PathParams { water: false, ..Default::default() }).unwrap();
    assert_eq!(wet(&dry), 0, "{:?}", dry.legs[0].points);
    assert!(dry.length > 1400.0, "{}", dry.length);
    // A goal in the water is out of its reach at once; standing in it, it walks out.
    let pond = Place { space: W, pos: [450.0, 150.0, 0.0] };
    let no_swim = PathParams { water: false, ..Default::default() };
    assert!(matches!(g.find_path(from, pond, &no_swim), Err(PathError::Unreachable { expanded: 0 })));
    assert!(g.find_path(pond, to, &no_swim).is_ok());
}

// Broken data (a damaged or badly made plugin): the graph builds, queries answer, and
// nothing panics or runs on for ever.

/// `grid` plus triangles whose corners are not finite, lie far out, or spread over many
/// cells (each its own three vertices, no neighbours).
fn with_bad_corners() -> NavMeshRecord {
    let mut m = grid(1, [0.0, 0.0], 4, 4, 100.0, &[], flat);
    let bad: [[[f32; 3]; 3]; 4] = [
        [[0.0, 0.0, 0.0], [f32::INFINITY, 0.0, 0.0], [0.0, 100.0, 0.0]],
        [[0.0, 0.0, 0.0], [100.0, 0.0, f32::NAN], [0.0, 100.0, 0.0]],
        [[1.0e30, 0.0, 0.0], [1.0e30, 100.0, 0.0], [-1.0e30, 0.0, 0.0]],
        [[0.0, 0.0, 0.0], [1.0e6, 0.0, 0.0], [0.0, 1.0e6, 0.0]],
    ];
    for c in bad {
        let first = m.vertices.len() as u16;
        m.vertices.extend(c);
        m.triangles.push(Triangle { v: [first, first + 1, first + 2], edges: [-1; 3], flags: 0, cover: 0 });
    }
    m
}

#[test]
fn triangles_with_broken_corners_are_left_out() {
    let g = graph(&[(with_bad_corners(), W)]);
    assert_eq!(g.stats.bad_triangles, 4);
    for t in 32..36 {
        assert!(g.tris[t].has(tri_flags::DELETED) && !g.usable(t as u32));
    }
    // The good part still works.
    let p = g.find_path(Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: W, pos: [350.0, 350.0, 0.0] }, &PathParams::default()).unwrap();
    assert!(on_mesh(&g, W, &p.legs[0].points, 10.0));
}

#[test]
fn indices_past_the_end_are_ignored() {
    let mut m = grid(1, [0.0, 0.0], 2, 2, 100.0, &[], flat);
    let n = m.triangles.len();
    m.triangles[0].v[0] = 999; // vertex past the end
    m.triangles[1].edges[0] = 999; // neighbour past the end
    m.triangles[2].edges[1] = -7; // negative neighbour
    // Linked edges: NVEX index past the list, a navmesh not loaded, a triangle past the end.
    m.triangles[3].flags |= 1 << 1;
    m.triangles[3].edges[1] = 50;
    link(&mut m, 4, 1, 0xDEAD, 0, link_kind::PORTAL);
    link(&mut m, 5, 1, 1, 60_000, link_kind::LEDGE_UP);
    m.cover_triangles.extend([0, 65_535]);
    m.door_links.push(DoorLink { door: 0xD1, triangle: 65_535 });
    // A navmesh with triangles and no vertices at all.
    let mut empty = NavMeshRecord { form_id: 2, ..Default::default() };
    empty.triangles.push(Triangle { v: [0, 1, 2], edges: [0, 1, 2], flags: 0b111, cover: 0 });
    let meshes = [(m, W), (empty, W)];
    let input: Vec<_> = meshes.iter().map(|(m, s)| (m, Some(0), Some(*s))).collect();
    let mut g = NavGraph::assemble(&input);
    g.join_doors(&input, |_| Some((0xD2, [f32::NAN, 0.0, 0.0], 0.0, W)));
    assert_eq!(g.tris.len(), n + 1);
    assert!(g.stats.unresolved_links >= 3);
    assert_eq!(g.stats.doors, 0);
    assert!(!g.usable(0) && !g.usable(n as u32));
    for t in 0..g.tris.len() as u32 {
        assert!(g.tris[t as usize].next.iter().all(|&x| x == NONE || (x as usize) < g.tris.len()));
    }
    // Most of the grid is broken: a path or a clean error, either is an answer.
    for (a, b) in [([150.0, 150.0, 0.0], [50.0, 150.0, 0.0]), ([50.0, 50.0, 0.0], [150.0, 150.0, 0.0])] {
        let _ = g.find_path(Place { space: W, pos: a }, Place { space: W, pos: b }, &PathParams { ledges: true, ..Default::default() });
        let _ = g.find_path(Place { space: W, pos: a }, Place { space: W, pos: b }, &PathParams::default());
    }
}

#[test]
fn queries_with_broken_numbers_answer_at_once() {
    let g = graph(&[(with_bad_corners(), W)]);
    let ok = [100.0, 100.0, 0.0];
    for p in [[f32::NAN, 0.0, 0.0], [f32::INFINITY, 0.0, 0.0], [0.0, f32::NEG_INFINITY, 0.0], [0.0, 0.0, f32::NAN]] {
        assert_eq!(g.nearest(W, p, 512.0), None, "{p:?}");
        assert!(g.triangles_near(W, p[0], p[1], 512.0).is_empty() || p[0].is_finite() && p[1].is_finite());
        let e = g.find_path(Place { space: W, pos: ok }, Place { space: W, pos: p }, &PathParams::default());
        assert!(matches!(e, Err(PathError::NoTriangle { .. })), "{p:?}: {e:?}");
    }
    // Huge, infinite and NaN radii.
    for r in [f32::MAX, f32::INFINITY, f32::NAN, -5.0] {
        let _ = g.nearest(W, [5000.0, 5000.0, 0.0], r);
        let _ = g.triangles_near(W, 100.0, 100.0, r);
        let _ = g.cover_edges_near(W, ok, r);
    }
    assert!(g.nearest(W, [5000.0, 5000.0, 0.0], f32::MAX).is_some());
    // Far-out but finite points (bucket indices at the ends of i32).
    for p in [[3.0e38, 3.0e38, 0.0], [-3.0e38, 0.0, 0.0]] {
        assert_eq!(g.nearest(W, p, f32::MAX), None);
        assert!(g.triangles_near(W, p[0], p[1], f32::MAX).is_empty());
    }
}

#[test]
fn obstacles_and_doors_with_broken_placement_do_not_hang() {
    let mut g = graph(&[(grid(1, [0.0, 0.0], 4, 4, 100.0, &[], flat), W)]);
    let o = |scale: f32, position: [f32; 3]| crate::obstacles::Obstacle {
        reference: 0x5B0,
        space: W,
        position,
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        scale,
        bounds: ([-50.0, -4.0, 0.0], [50.0, 4.0, 90.0]),
        initially_enabled: true,
    };
    for (s, p) in [(f32::INFINITY, [200.0, 200.0, 0.0]), (1.0e30, [200.0, 200.0, 0.0]), (f32::NAN, [200.0, 200.0, 0.0]), (1.0, [f32::NAN, 0.0, 0.0]), (1.0, [1.0e30, 1.0e30, 0.0])] {
        g.set_obstacles(vec![o(s, p)], |o| o.initially_enabled);
    }
}

// ---- the feet's height against the path (#1500; docs/design/navmesh.md §4) --------------

fn straight_path(points: Vec<[f32; 3]>, hops: Vec<crate::LedgeHop>) -> crate::Path {
    crate::Path {
        legs: vec![crate::Leg { space: W, points, corridor: vec![], door: None, door_stops: vec![], hops }],
        length: 0.0,
        expanded: 0,
        start_snap: 0.0,
        goal_snap: 0.0,
    }
}

#[test]
fn feet_straight_below_or_above_the_end_of_a_path_have_not_arrived() {
    let path = straight_path(vec![[0.0, 0.0, 100.0], [100.0, 0.0, 100.0]], vec![]);
    assert_eq!(Follower::new(path.clone()).steer([100.0, 0.0, 100.0]), Steer::Arrived);
    assert_eq!(Follower::new(path.clone()).steer([100.0, 0.0, 0.0]), Steer::OffPath);
    assert_eq!(Follower::new(path.clone()).steer([100.0, 0.0, 200.0]), Steer::OffPath);
    // A swimmer's height is not judged.
    assert_eq!(Follower::new(path).steer_any_height([100.0, 0.0, 0.0]), Steer::Arrived);
}

#[test]
fn the_tolerance_ends_at_its_number() {
    let path = straight_path(vec![[0.0, 0.0, 100.0], [100.0, 0.0, 100.0]], vec![]);
    for (dz, arrived) in [(HEIGHT_TOLERANCE, true), (-HEIGHT_TOLERANCE, true), (HEIGHT_TOLERANCE + 0.5, false), (-HEIGHT_TOLERANCE - 0.5, false)] {
        let s = Follower::new(path.clone()).steer([100.0, 0.0, 100.0 + dz]);
        assert_eq!(s == Steer::Arrived, arrived, "{dz}: {s:?}");
        assert_eq!(s == Steer::OffPath, !arrived, "{dz}: {s:?}");
    }
}

/// The real way from the floor up to the deck over it, and a follower that has walked it
/// up to its last point but one (feet at each corner).
fn up_to_the_deck(g: &NavGraph, to: [f32; 3]) -> Follower {
    let path = g.find_path(Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: W, pos: to }, &PathParams::default()).unwrap();
    assert!(path.legs[0].points.len() >= 3, "{:?}", path.legs[0].points);
    let mut f = Follower::new(path);
    let last = f.path.legs[0].points.len() - 1;
    while f.next < last {
        let at = f.path.legs[0].points[f.next];
        assert!(matches!(f.steer(at), Steer::Walk { .. }));
    }
    f
}

#[test]
fn a_real_path_is_not_finished_from_the_floor_under_its_end() {
    let g = floor_with_deck_above();
    let to = [150.0, 350.0, 200.0];
    let mut f = up_to_the_deck(&g, to);
    let next = f.next;
    // Physically on the floor, right under the end: not arrived, and nothing consumed.
    assert_eq!(f.steer([150.0, 350.0, 0.0]), Steer::OffPath);
    assert_eq!(f.next, next);
    // A way that fits from there: back to the ramp, and up, not straight up.
    let n = f.replanned(&g, Place { space: W, pos: [150.0, 350.0, 0.0] }, &PathParams::default()).unwrap();
    assert!(n.path.legs[0].points.iter().any(|q| q[0] > 700.0), "{:?}", n.path.legs[0].points);
    assert_eq!(n.path.legs[0].points[0], [150.0, 350.0, 0.0]);
    // On the deck it has arrived.
    assert_eq!(f.steer([150.0, 350.0, 200.0]), Steer::Arrived);
}

#[test]
fn a_corner_is_not_reached_from_another_floor() {
    let g = floor_with_deck_above();
    let mut f = up_to_the_deck(&g, [150.0, 350.0, 200.0]);
    // Back at a corner high up on the way (the path's first point over 150 up).
    let pts = f.path.legs[0].points.clone();
    let i = (1..pts.len() - 1).find(|&i| pts[i][2] > 150.0).expect("a corner up there");
    let q = pts[i];
    f.next = i;
    let next = f.next;
    // Within the corner's radius, 100 off on either side.
    for dz in [-100.0, 100.0] {
        assert_eq!(f.steer([q[0] + 5.0, q[1], q[2] + dz]), Steer::OffPath, "{dz}");
        assert_eq!(f.next, next);
    }
    // Beyond it along its segment, seen from above, where the actor on the way counts as
    // past it: 100 off it is not past it, and heads back to the corner.
    let p = pts[i - 1];
    let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
    let len = dx.hypot(dy);
    let beyond = [q[0] + dx / len * 30.0, q[1] + dy / len * 30.0, q[2]];
    match f.steer([beyond[0], beyond[1], beyond[2] - 100.0]) {
        Steer::Walk { target, .. } => assert_eq!(target, q),
        s => panic!("{s:?}"),
    }
    assert_eq!(f.next, next);
    // On its height it counts as past it, as ever.
    assert!(matches!(f.steer(beyond), Steer::Walk { target, .. } if target != q));
    assert!(f.next > next);
}

#[test]
fn a_door_is_not_used_from_another_floor() {
    let g = corridor_with_door(None);
    let from = Place { space: W, pos: [50.0, 50.0, 0.0] };
    let to = Place { space: W, pos: [950.0, 50.0, 0.0] };
    let p = g.find_path(from, to, &PathParams::default()).unwrap();
    let stop = p.legs[0].door_stops[0];
    let at = p.legs[0].points[stop.point];
    let mut f = Follower::new(p);
    let below = [at[0] - 10.0, at[1], at[2] - 100.0];
    let beside = [at[0] - 10.0, at[1], at[2]];
    // First use: the script is not asked and nothing starts from below it.
    assert_eq!(f.door_ahead(below, 48.0), None);
    assert_eq!(f.door_ahead(beside, 48.0), Some(0xD00));
    assert_eq!(f.steer(below), Steer::OffPath);
    assert!(!f.holding);
    // Behind the corner walked to, with no hold yet: from below it holds nothing.
    let mut past = f.clone();
    past.next = stop.point + 1;
    assert_eq!(past.steer(below), Steer::OffPath);
    assert!(!past.holding);
    // Held, it stays held wherever the opening door pushes the actor.
    assert_eq!(f.steer(beside), Steer::OpenDoor { door: 0xD00, at });
    assert!(f.holding);
    assert_eq!(f.steer(below), Steer::OpenDoor { door: 0xD00, at });
    assert_eq!(f.door_ahead(below, 48.0), Some(0xD00));
}

/// A way that ends at a load door in the far corner of a flat mesh, into another space.
fn to_a_load_door() -> (NavGraph, crate::Path) {
    let inside = Space::Interior(0x77);
    let mut a = grid(1, [0.0, 0.0], 4, 4, 100.0, &[], flat);
    let b = grid(2, [5000.0, 5000.0], 3, 3, 100.0, &[], |_, _| 50.0);
    let ta = (0..a.triangles.len()).find(|&t| a.triangles[t].v.map(|i| a.vertices[i as usize]).iter().all(|p| p[0] >= 300.0 && p[1] >= 300.0)).unwrap();
    a.door_links.push(DoorLink { door: 0xD1, triangle: ta as u16 });
    a.triangles[ta].flags |= tri_flags::DOOR;
    let meshes = [(a, W), (b, inside)];
    let input: Vec<_> = meshes.iter().map(|(m, s)| (m, Some(0), Some(*s))).collect();
    let mut g = NavGraph::assemble(&input);
    g.join_doors(&input, |door| (door == 0xD1).then_some((0xD2, [5050.0, 5030.0, 50.0], 0.5, inside)));
    g.place_doors(|d| (d == 0xD1).then_some([360.0, 700.0, 0.0]));
    let p = g.find_path(Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: inside, pos: [5250.0, 5250.0, 50.0] }, &PathParams::default()).unwrap();
    (g, p)
}

#[test]
fn a_load_door_needs_the_feet_at_its_height_on_every_frame_of_facing_it() {
    let (_, p) = to_a_load_door();
    let end = *p.legs[0].points.last().unwrap();
    let mut f = Follower::new(p);
    let last = f.path.legs[0].points.len() - 1;
    f.next = last;
    let (near, below) = ([end[0] - 10.0, end[1], end[2]], [end[0] - 10.0, end[1], end[2] - 100.0]);
    // From below nothing is asked and nothing used.
    assert_eq!(f.door_ahead(below, 48.0), None);
    assert_eq!(f.steer(below), Steer::OffPath);
    assert_eq!(f.next, last);
    // At the door: the end is consumed, and it stands there frame after frame facing it.
    assert_eq!(f.door_ahead(near, 48.0), Some(0xD1));
    assert!(matches!(f.steer(near), Steer::Door(_)));
    assert!(matches!(f.steer(near), Steer::Door(_)));
    assert!(f.next > last);
    // Moved off its height while it turned: it does not go through.
    assert_eq!(f.steer(below), Steer::OffPath);
    assert!(matches!(f.steer(near), Steer::Door(_)));
}

const TAN_60: f32 = 1.732_050_8;

#[test]
fn the_last_steps_on_a_60_degree_slope_are_on_the_path() {
    // A wide radius (a door's is 48), so the feet are far below the end on the slope: 83
    // units, past the tolerance from the point itself.
    let end = [100.0, 0.0, 100.0 * TAN_60];
    let path = straight_path(vec![[0.0, 0.0, 0.0], end], vec![]);
    let mut f = Follower::new(path);
    f.goal_radius = 48.0;
    let on_slope = [52.0, 0.0, 52.0 * TAN_60];
    assert!(end[2] - on_slope[2] > HEIGHT_TOLERANCE);
    assert_eq!(f.clone().steer(on_slope), Steer::Arrived);
    // The slope is no allowance: 100 off it is off the path.
    assert_eq!(f.clone().steer([52.0, 0.0, 52.0 * TAN_60 - 100.0]), Steer::OffPath);
    assert_eq!(f.steer([52.0, 0.0, 52.0 * TAN_60 + 100.0]), Steer::OffPath);
}

#[test]
fn walking_down_beyond_a_corner_is_on_the_path() {
    // Flat, then 60 degrees down: 50 past the corner it is 87 below it.
    let corner = [100.0, 0.0, 0.0];
    let path = straight_path(vec![[0.0, 0.0, 0.0], corner, [300.0, 0.0, -200.0 * TAN_60]], vec![]);
    let mut f = Follower::new(path.clone());
    assert!(matches!(f.steer([150.0, 0.0, -50.0 * TAN_60]), Steer::Walk { target, .. } if target[0] == 300.0));
    assert_eq!(f.next, 2);
    // 100 below that slope it is on no segment here: it heads back to the corner.
    let mut f = Follower::new(path);
    assert!(matches!(f.steer([150.0, 0.0, -50.0 * TAN_60 - 100.0]), Steer::Walk { target, .. } if target == corner));
    assert_eq!(f.next, 1);
}

#[test]
fn slope_reversals_at_a_corner_are_on_the_path() {
    // Up 60 degrees to a ridge and down again: past it on the far side.
    let top = [100.0, 0.0, 100.0 * TAN_60];
    let path = straight_path(vec![[0.0, 0.0, 0.0], top, [200.0, 0.0, 0.0]], vec![]);
    let mut f = Follower::new(path);
    assert!(matches!(f.steer([140.0, 0.0, 60.0 * TAN_60]), Steer::Walk { target, .. } if target[0] == 200.0));
    // Down into a valley and up again: within the radius on the near side, and past it.
    let bottom = [100.0, 0.0, 0.0];
    let path = straight_path(vec![[0.0, 0.0, 100.0 * TAN_60], bottom, [200.0, 0.0, 100.0 * TAN_60]], vec![]);
    let mut f = Follower::new(path.clone());
    assert!(matches!(f.steer([90.0, 0.0, 10.0 * TAN_60]), Steer::Walk { target, .. } if target[0] == 200.0));
    let mut f = Follower::new(path);
    assert!(matches!(f.steer([140.0, 0.0, 40.0 * TAN_60]), Steer::Walk { target, .. } if target[0] == 200.0));
}

#[test]
fn a_segment_with_no_length_seen_from_above_says_nothing_of_a_slope() {
    // The point itself is what counts: nothing interpolated between two heights.
    let path = straight_path(vec![[100.0, 0.0, 300.0], [100.0, 0.0, 100.0]], vec![]);
    assert_eq!(Follower::new(path.clone()).steer([100.0, 0.0, 100.0]), Steer::Arrived);
    assert_eq!(Follower::new(path).steer([100.0, 0.0, 200.0]), Steer::OffPath);
}

#[test]
fn hop_endpoints_close_seen_from_above_do_not_skip_the_flight() {
    use crate::LedgeHop;
    // Off a ledge 100 high whose landing is 2 away seen from above.
    let down = straight_path(vec![[0.0, 0.0, 100.0], [50.0, 0.0, 100.0], [52.0, 0.0, 0.0], [150.0, 0.0, 0.0]], vec![LedgeHop { point: 1, up: false }]);
    let mut f = Follower::new(down.clone());
    // At the take-off: consumed, and the landing is still to come, whatever its distance.
    assert!(matches!(f.steer([50.0, 0.0, 100.0]), Steer::Walk { target, .. } if target[2] == 0.0));
    assert_eq!(f.next, 2);
    assert!(f.hop().is_some());
    // In the air above it: not judged a fall, and it lands first.
    assert!(f.fits([52.0, 0.0, 60.0]));
    assert!(matches!(f.steer([52.0, 0.0, 60.0]), Steer::Walk { target, .. } if target[2] == 0.0));
    // Landed: on.
    assert!(matches!(f.steer([52.0, 0.0, 0.0]), Steer::Walk { target, .. } if target[0] == 150.0));
    assert!(f.hop().is_none());
    // Below the landing it fell past it.
    let mut f = Follower::new(down.clone());
    f.next = 2;
    assert_eq!(f.steer([52.0, 0.0, -100.0]), Steer::OffPath);
    // Under the take-off, the ledge is not taken off from: no flight on another floor.
    let mut f = Follower::new(down);
    assert_eq!(f.steer([50.0, 0.0, 0.0]), Steer::OffPath);
    assert_eq!(f.next, 1);
    assert!(f.hop().is_none());
    // Up onto one: from below the landing it jumps first; above it, it is not on the way.
    let up = straight_path(vec![[0.0, 0.0, 0.0], [50.0, 0.0, 0.0], [52.0, 0.0, 60.0], [150.0, 0.0, 60.0]], vec![LedgeHop { point: 1, up: true }]);
    let mut f = Follower::new(up);
    assert!(matches!(f.steer([50.0, 0.0, 0.0]), Steer::Walk { target, .. } if target[2] == 60.0));
    assert_eq!(f.next, 2);
    assert_eq!(f.steer([52.0, 0.0, 160.0]), Steer::OffPath);
}

#[test]
fn a_walker_flies_a_ledge_whose_ends_are_close_seen_from_above() {
    use crate::LedgeHop;
    let top = grid(1, [0.0, 0.0], 1, 1, 51.0, &[], |_, _| 100.0);
    let low = grid(2, [52.0, 0.0], 2, 1, 100.0, &[], flat);
    let g = graph(&[(top, W), (low, W)]);
    let path = straight_path(vec![[5.0, 25.0, 100.0], [50.0, 25.0, 100.0], [52.0, 25.0, 0.0], [150.0, 25.0, 0.0]], vec![LedgeHop { point: 1, up: false }]);
    let mut w = Walker::new(W, [5.0, 25.0, 100.0], std::f32::consts::FRAC_PI_2, 100.0);
    w.follower = Some(Follower::new(path));
    let mut from = None;
    for _ in 0..1000 {
        let ev = w.update(&g, 1.0 / 60.0);
        assert_ne!(ev, WalkEvent::PathLost, "at {:?}", w.feet);
        from = from.or(w.air.map(|a| a.flight.from));
        if w.follower.is_none() {
            break;
        }
    }
    assert!(w.follower.is_none());
    assert_eq!((w.flights, w.landings), (1, 1));
    assert_eq!(from, Some([50.0, 25.0, 100.0]));
    assert!((w.feet[2]).abs() < 1.0, "{:?}", w.feet);
}

#[test]
fn a_goal_above_the_navmesh_is_arrived_at_where_it_was_snapped() {
    let g = graph(&[(grid(1, [0.0, 0.0], 4, 4, 100.0, &[], flat), W)]);
    let to = Place { space: W, pos: [350.0, 350.0, 300.0] };
    let mut w = Walker::new(W, [50.0, 50.0, 0.0], 0.0, 100.0);
    assert!(w.go_to(&g, to, &PathParams::default()).unwrap().goal_snap >= 300.0);
    assert_eq!(w.follower.as_ref().unwrap().destination().unwrap().pos[2], 0.0);
    let mut last = WalkEvent::Idle;
    for _ in 0..1000 {
        last = w.update(&g, 0.05);
        if w.follower.is_none() {
            break;
        }
    }
    assert_eq!(last, WalkEvent::Arrived);
}

#[test]
fn a_walker_put_on_the_floor_under_its_goal_stands_there_and_takes_the_ramp() {
    let g = floor_with_deck_above();
    let to = Place { space: W, pos: [150.0, 350.0, 200.0] };
    let params = PathParams::default();
    let mut w = Walker::new(W, [650.0, 350.0, 200.0], 0.0, 100.0);
    w.go_to(&g, to, &params).unwrap();
    // Physics left it on the floor under its goal (`residents.rs` hands over `tri` unset).
    let under = [150.0, 350.0, 0.0];
    (w.feet, w.tri) = (under, NONE);
    assert_eq!(w.update(&g, 0.05), WalkEvent::PathLost);
    assert_eq!(w.feet, under, "no teleport");
    assert!(w.follower.is_some(), "the walk is not over");
    let length = w.replan(&g, &params).unwrap();
    assert!(length > 1000.0, "{length}");
    let (mut high_x, mut before) = (0.0f32, under);
    for _ in 0..5000 {
        let ev = w.update(&g, 0.05);
        assert_ne!(ev, WalkEvent::PathLost, "at {:?}", w.feet);
        high_x = high_x.max(w.feet[0]);
        // Up the ramp, no faster than a slope of 1 in 2 lifts it in a step.
        assert!((w.feet[2] - before[2]).abs() <= 0.5 * (w.feet[0] - before[0]).hypot(w.feet[1] - before[1]) + 1.0, "{before:?} to {:?}", w.feet);
        before = w.feet;
        if w.follower.is_none() {
            break;
        }
    }
    assert!(w.follower.is_none());
    assert!(high_x > 700.0, "not by the ramp: {high_x}");
    assert_eq!(w.feet, to.pos);
}

#[test]
fn a_walker_off_the_navmesh_goes_onto_it_and_walks_on() {
    let g = floor_with_deck_above();
    // Placed 60 above the deck, right at a corner: its first update puts it on the deck.
    let to = Place { space: W, pos: [150.0, 350.0, 200.0] };
    let mut w = Walker::new(W, [650.0, 350.0, 200.0], 0.0, 100.0);
    w.go_to(&g, to, &PathParams::default()).unwrap();
    w.feet = [150.0, 350.0, 260.0];
    w.tri = NONE;
    assert_ne!(w.update(&g, 0.05), WalkEvent::PathLost);
    assert_eq!(w.feet[2], 200.0);
    assert!(w.follower.is_none(), "it stands at its goal");
}

#[test]
fn a_walker_with_no_navmesh_under_it_keeps_to_its_path() {
    // Nothing near within the walker's search: its step takes the path's height, as ever.
    let g = graph(&[(grid(1, [0.0, 0.0], 4, 4, 100.0, &[], flat), W)]);
    let to = Place { space: W, pos: [350.0, 50.0, 0.0] };
    let mut w = Walker::new(W, [50.0, 50.0, 0.0], 0.0, 100.0);
    w.go_to(&g, to, &PathParams::default()).unwrap();
    (w.feet, w.tri) = ([350.0, 50.0, -500.0], NONE);
    assert_ne!(w.update(&g, 0.05), WalkEvent::PathLost);
    assert_eq!(w.feet[2], 0.0);
}

#[test]
fn a_new_way_cannot_start_on_a_floor_over_the_feet() {
    // The deck alone: nothing under it to walk on, feet 100 below it.
    let g = graph(&[(grid(1, [0.0, 0.0], 4, 4, 100.0, &[], |_, _| 200.0), W)]);
    let params = PathParams::default();
    let f = Follower::new(g.find_path(Place { space: W, pos: [50.0, 50.0, 200.0] }, Place { space: W, pos: [350.0, 50.0, 200.0] }, &params).unwrap());
    assert!(f.replanned(&g, Place { space: W, pos: [350.0, 50.0, 100.0] }, &params).is_err());
    // Within the tolerance below it: the feet are on it.
    let n = f.replanned(&g, Place { space: W, pos: [250.0, 50.0, 170.0] }, &params).unwrap();
    assert_eq!(n.height_offset, 0.0);
    // Too near over them for a floor they stand under: a navmesh above its collision.
    let mut n = f.replanned(&g, Place { space: W, pos: [340.0, 50.0, 150.0] }, &params).unwrap();
    assert_eq!(n.height_offset, -50.0);
    assert_eq!(n.clone().steer([350.0, 50.0, 150.0]), Steer::Arrived);
    assert_eq!(n.clone().steer([350.0, 50.0, 200.0]), Steer::Arrived);
    assert_eq!(n.steer([350.0, 50.0, 100.0]), Steer::OffPath);
}

#[test]
fn feet_on_something_the_navmesh_does_not_know_may_walk_off_it() {
    let g = graph(&[(grid(1, [0.0, 0.0], 4, 4, 100.0, &[], flat), W)]);
    let params = PathParams::default();
    let mut old = Follower::new(g.find_path(Place { space: W, pos: [50.0, 50.0, 0.0] }, Place { space: W, pos: [350.0, 50.0, 0.0] }, &params).unwrap());
    old.goal_radius = 8.0;
    // 60 above the only surface, no other floor anywhere: the path allows for that, on that side.
    let on_a_rock = Place { space: W, pos: [340.0, 50.0, 60.0] };
    let mut n = old.replanned(&g, on_a_rock, &params).unwrap();
    assert_eq!(n.height_offset, 60.0);
    assert_eq!(n.goal_radius, 8.0);
    assert!(matches!(n.steer(on_a_rock.pos), Steer::Walk { .. }));
    assert_eq!(n.clone().steer([350.0, 50.0, 60.0]), Steer::Arrived);
    assert_eq!(n.clone().steer([350.0, 50.0, -60.0]), Steer::OffPath);
    assert_eq!(n.steer([350.0, 50.0, 0.0]), Steer::Arrived);
}

#[test]
fn plans_made_again_are_bounded_until_the_actor_has_got_on() {
    let mut r = PathRecovery::default();
    let here = [100.0, 100.0, 0.0];
    for _ in 0..RECOVERY_TRIES {
        assert!(r.allow(here, 0.0));
    }
    assert!(!r.allow(here, 1.0));
    assert!(!r.allow([150.0, 100.0, 0.0], 2.0), "moved about the same spot");
    // Further on, it starts over; so it does a good while later.
    assert!(r.allow([100.0 + RECOVERY_RESET_DISTANCE, 100.0, 0.0], 3.0));
    let mut r = PathRecovery::default();
    for _ in 0..RECOVERY_TRIES {
        assert!(r.allow(here, 0.0));
    }
    assert!(r.allow(here, RECOVERY_RESET_SECONDS));
}

// ---- controls: feet on the mesh are as before -------------------------------------------

/// A walker of `from` to `to` on `g`, with no path lost on the way, to its goal.
fn walks_to(g: &NavGraph, from: [f32; 3], to: [f32; 3], speed: f32, dt: f32, params: &PathParams) -> Walker {
    let mut w = Walker::new(W, from, 0.0, speed);
    w.go_to(g, Place { space: W, pos: to }, params).unwrap();
    for _ in 0..20_000 {
        let ev = w.update(g, dt);
        assert_ne!(ev, WalkEvent::PathLost, "at {:?}", w.feet);
        if w.follower.is_none() {
            return w;
        }
    }
    panic!("did not arrive: {:?}", w.feet);
}

#[test]
fn walks_on_the_navmesh_do_not_lose_their_paths() {
    // Up and down a 60 degree valley, at any step.
    let valley = |x: f32, _y: f32| (x - 300.0).abs() * TAN_60;
    let g = graph(&[(grid(1, [0.0, 0.0], 12, 4, 50.0, &[], valley), W)]);
    let (from, to) = ([25.0, 100.0, valley(25.0, 0.0)], [575.0, 100.0, valley(575.0, 0.0)]);
    for (speed, dt) in [(85.3, 1.0 / 60.0), (344.0, 1.0 / 30.0), (100.0, 0.25)] {
        let w = walks_to(&g, from, to, speed, dt, &PathParams::default());
        assert!((w.feet[2] - valley(w.feet[0], 0.0)).abs() < 1.0, "{:?}", w.feet);
    }
    // Up the ramp to the deck over the floor.
    let g = floor_with_deck_above();
    for dt in [1.0 / 60.0, 0.25] {
        let w = walks_to(&g, [150.0, 350.0, 0.0], [150.0, 350.0, 200.0], 100.0, dt, &PathParams::default());
        assert_eq!(w.feet, [150.0, 350.0, 200.0]);
    }
}
