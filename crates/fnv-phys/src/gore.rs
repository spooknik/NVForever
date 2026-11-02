//! Gore pieces (docs/spec-notes/dismemberment.md §6): the bits an exploded body part
//! leaves, each a small dynamic box that flies, bounces and settles.
//!
//! - A piece's colliders carry [`Owner::Actor`] of the dead actor on the DEAD_BIP layer
//!   with part [`GORE_PART`], so rays find the corpse through it (the crosshair names the
//!   actor, activating a piece searches the corpse) and characters walk through it as they
//!   do through corpses (they pass the actor volumes).
//! - Contacts [design]: with the world and with other pieces; not with characters, living
//!   actors' hit volumes or ragdolls (`crate::ragdoll::RagdollHooks`).

use crate::convert::vec3;
use crate::layers;
use crate::world::{body_state, BodyState, Owner, PhysWorld};
use rapier3d::math::{Matrix, Pose, Rotation};
use rapier3d::prelude::*;

/// The part number a gore piece's colliders report (`Owner::Actor::part`).
pub const GORE_PART: u8 = 253;

/// How to start a piece. Game units, row-major rotation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoreStart {
    /// The engine's id for the piece.
    pub id: u32,
    /// The dead actor the piece belongs to.
    pub actor: u32,
    pub translation: [f32; 3],
    pub rotation: [f32; 9],
    /// The box's half extents and its centre in the piece's frame.
    pub half_extents: [f32; 3],
    pub center: [f32; 3],
    pub linvel: [f32; 3],
    /// Radians per second.
    pub angvel: [f32; 3],
    /// Kilograms.
    pub mass: f32,
}

/// Friction and bounce of a piece [design].
pub const GORE_FRICTION: f32 = 0.8;
pub const GORE_RESTITUTION: f32 = 0.25;

impl PhysWorld {
    /// Add (or replace) a gore piece.
    pub fn add_gore(&mut self, s: &GoreStart) {
        self.remove_gore(s.id);
        let k = self.k;
        let m = Matrix::from_cols_array(&s.rotation).transpose();
        let q = Rotation::from_mat3(&m).normalize();
        let q = if q.is_finite() { q } else { Rotation::IDENTITY };
        let owner = Owner::Actor { form_id: s.actor, layer: layers::DEAD_BIP, part: GORE_PART }.encode();
        let body = RigidBodyBuilder::dynamic()
            .pose(Pose::from_parts(vec3(s.translation) * k, q))
            .linvel(vec3(s.linvel) * k)
            .angvel(vec3(s.angvel))
            .linear_damping(0.1)
            .angular_damping(0.5)
            .ccd_enabled(true)
            .user_data(owner);
        let h = self.insert_body(body);
        let he = s.half_extents.map(|v| (v * k).max(0.005));
        let c = ColliderBuilder::cuboid(he[0], he[1], he[2])
            .position(Pose::from_parts(vec3(s.center) * k, Rotation::IDENTITY))
            .friction(GORE_FRICTION)
            .restitution(GORE_RESTITUTION)
            .active_hooks(ActiveHooks::FILTER_CONTACT_PAIRS)
            .user_data(owner);
        self.insert_collider(c, Some(h));
        if s.mass > 0.0 && s.mass.is_finite() {
            self.set_mass(h, s.mass);
        }
        self.gore.insert(s.id, h);
        self.mark_dirty(crate::refresh::DirtyReasons::GORE);
    }

    /// Remove a gore piece.
    pub fn remove_gore(&mut self, id: u32) -> bool {
        let Some(h) = self.gore.remove(&id) else { return false };
        self.remove_body(h);
        true
    }

    /// A piece's pose and velocity now.
    pub fn gore_state(&self, id: u32) -> Option<BodyState> {
        let b = self.w.bodies.get(*self.gore.get(&id)?)?;
        Some(body_state(b, self.k))
    }

    /// The ids of the gore pieces in the world.
    pub fn gore_pieces(&self) -> impl Iterator<Item = u32> + '_ {
        self.gore.keys().copied()
    }

    /// Change a piece's velocity by `dv` (game units per second): a shot or blast on it.
    pub fn push_gore(&mut self, id: u32, dv: [f32; 3]) -> bool {
        let Some(&h) = self.gore.get(&id) else { return false };
        let k = self.k;
        let Some(b) = self.w.bodies.get_mut(h) else { return false };
        b.set_linvel(b.linvel() + vec3(dv) * k, true);
        self.struck.push(h);
        true
    }

    /// Whether this body is a gore piece.
    pub(crate) fn is_gore(owner: Option<Owner>) -> bool {
        matches!(owner, Some(Owner::Actor { part: GORE_PART, .. }))
    }
}
