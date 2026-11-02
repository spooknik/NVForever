//! Physics for Layer 4: a rapier3d world built from terrain (LAND heights, via
//! `fnv-world`) and placed references' collision objects (`fnv_collide` shapes), with
//! ray and shape queries, a kinematic capsule character controller, dynamic bodies for
//! clutter and deterministic fixed-step stepping. Headless; no Bevy.
//!
//! - [`convert`]: `fnv_collide::Shape` trees to rapier shapes
//! - [`terrain`]: one heightfield per exterior cell
//! - [`world`]: the world, bodies, stepping, queries
//! - [`character`]: the character controller
//! - [`air`]: horizontal motion in the air and after landings
//! - [`arrival`]: where an actor comes to rest after a teleport (shared by the tools and
//!   the game)
//! - [`layers`]: Havok collision layer numbers and names
//! - [`primitive`]: collision primitives placed on references (CollisionMarker boxes)
//! - [`placed`]: the collision objects of a placed reference (shared by the tools and the
//!   game)
//! - [`ragdoll`]: ragdolls, the jointed bodies of dead actors
//! - [`grab`]: a body held by the player's grab
//! - [`water`]: water areas (cell water surfaces) the character swims in
//! - [`buoyancy`]: ragdolls and gore pieces float in water
//! - [`refresh`]: why queries had to be brought up to date between steps, counted
//! - [`swim`]: swimming speeds (the stroke cycle)
//!
//! Units and axes are the game's: lengths in game units, Z up, rotations row-major 3×3
//! rotating column vectors (as in `fnv_collide::shape` and `fnv_world::transform`).
//! Design: docs/design/layer4.md §2.8 and §7 (workstream 2); facts, hypotheses and the
//! CLI contract: docs/spec-notes/physics.md.

pub mod actors;
pub mod air;
pub mod arrival;
pub mod buoyancy;
pub mod character;
pub mod convert;
pub mod gore;
pub mod grab;
pub mod layers;
pub mod placed;
pub mod primitive;
pub mod ragdoll;
pub mod refresh;
pub mod swim;
pub mod terrain;
pub mod water;
pub mod world;
pub mod work;

pub use actors::{HitVolume, PartCapsule};
pub use arrival::Arrival;
pub use character::{Character, CharacterInput, CharacterParams, CharacterStep, Push};
pub use convert::ConvertError;
pub use grab::{Grip, GripOn, HoldParams};
pub use layers::LayerSet;
pub use ragdoll::{RagdollReport, RagdollStart};
pub use refresh::{DirtyReasons, RefreshStats};
pub use placed::{ref_collision, ref_collision_with, ModelCollision, RefCollision, RefCollisionOptions};
pub use swim::Stroke;
pub use water::{PlacedWater, WaterArea};
pub use world::{
    AddReport, BodyState, Filter, Key, Owner, PhysParams, PhysWorld, PreparedRef, PreparedTerrain, Queries, RayHit, RefPose, Stats, Stepper,
    SweepHit, TERRAIN_SHELL,
};
