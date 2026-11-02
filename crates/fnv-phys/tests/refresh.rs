//! Bringing queries up to date between steps (`PhysWorld::update_queries`): the reasons
//! each refresh is counted by (`fnv_phys::refresh`). Hand-built worlds, no game data.

use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_phys::*;
use fnv_world::land::SIDE;

const ID: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

fn object(shape: Shape, motion: Motion) -> CollisionObject {
    CollisionObject {
        node: None,
        node_block: 0,
        node_transform: Iso::IDENTITY,
        body_transform: Iso::IDENTITY,
        motion,
        layer: 1,
        mass: 10.0,
        friction: 0.5,
        restitution: 0.0,
        shape: shape.into(),
        shell: 0.0,
        material: None,
    }
}

fn at(form_id: u32, position: [f32; 3]) -> RefPose {
    RefPose { form_id, position, rotation: ID, scale: 1.0 }
}

fn ground() -> PhysWorld {
    let mut w = PhysWorld::new(PhysParams::default());
    for x in -1..=0 {
        for y in -1..=0 {
            w.add_terrain((x, y), &vec![0.0; SIDE * SIDE]);
        }
    }
    w
}

fn person(feet: [f32; 3]) -> HitVolume {
    HitVolume { feet, radius: 20.0, height: 128.0, dead: false }
}

fn arm(feet: [f32; 3], lift: f32) -> Vec<PartCapsule> {
    vec![
        PartCapsule { part: 0, a: [feet[0], feet[1], feet[2] + 40.0], b: [feet[0], feet[1], feet[2] + 100.0], radius: 15.0 },
        PartCapsule { part: 1, a: [feet[0] + 20.0, feet[1], feet[2] + 90.0], b: [feet[0] + 50.0, feet[1], feet[2] + 90.0 + lift], radius: 5.0 },
    ]
}

#[test]
fn refreshes_are_counted_by_the_reasons_that_called_for_them() {
    let mut w = ground();
    assert_eq!(w.pending_refresh(), DirtyReasons::TERRAIN);
    w.update_queries();
    assert!(w.pending_refresh().is_empty());
    // Up to date: no second refresh.
    w.update_queries();
    assert_eq!(w.refresh_stats().full, 1);
    assert_eq!(w.refresh_stats().by_reasons[&DirtyReasons::TERRAIN], 1);

    w.set_actor_parts(0x10, person([200.0, 0.0, 0.0]), &arm([200.0, 0.0, 0.0], 0.0));
    assert!(w.pending_refresh().contains(DirtyReasons::ACTOR_PARTS));
    w.update_queries();
    w.set_actor_parts(0x10, person([200.0, 0.0, 0.0]), &arm([200.0, 0.0, 0.0], 10.0));
    w.add_ref(&at(7, [0.0, 0.0, 50.0]), &[object(Shape::Sphere { radius: 5.0 }, Motion::Static)]);
    assert_eq!(w.pending_refresh(), DirtyReasons::ACTOR_PARTS | DirtyReasons::BODIES);
    w.update_queries();
    let s = w.refresh_stats();
    assert_eq!(s.by_reasons[&(DirtyReasons::ACTOR_PARTS | DirtyReasons::BODIES)], 1);
    assert_eq!(s.full, s.by_reasons.values().sum::<u64>());

    // A step brings everything up to date without counting a refresh.
    w.set_actor_volume(0x11, person([-200.0, 0.0, 0.0]));
    let before = w.refresh_stats().full;
    w.step();
    assert!(w.pending_refresh().is_empty());
    w.update_queries();
    assert_eq!(w.refresh_stats().full, before);
    // A snapshot keeps the counts.
    assert_eq!(w.snapshot().refresh_stats(), w.refresh_stats());
}
