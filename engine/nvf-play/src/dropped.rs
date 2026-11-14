//! Dropped items: references the simulation creates at run time (`World::drop_item`,
//! FormIDs from 0xFF000000). They are not in the plugins, so render streaming
//! (`engine/nvforever`) and the placed-reference paths know nothing of them; this module
//! draws them and `crate::phys` gives them collision.
//!
//! - Drawing: every frame the created references the simulation holds are compared with
//!   what is drawn ([`plan`]). A present one whose render cell is loaded gets its base
//!   form's model, at the simulation's pose, as children of that cell's entity (the
//!   [`RenderCell`] nvforever puts on each cell it streams in), so it goes when render
//!   streaming drops the cell or a door reloads everything, and comes back with the cell.
//!   Its mesh entities carry `RefVisual(id)`, so the crosshair's render-bounds fallback
//!   finds it like a placed item. A taken one (`RefState::deleted`) is despawned; one
//!   that moved (falling, settling, pushed) follows the simulation's pose. Markers and
//!   bases of a type placed references are not drawn for (PlaceAtMe of an explosion) draw
//!   nothing ([`draws_model`]).
//! - Physics (`crate::phys`): its model's collision objects, dynamic when the model says
//!   so, loaded and unloaded with its physics cell; while it moves the body's pose is
//!   written back to the simulation's reference ([`ref_pose_from_body`]), so a cell
//!   reload, a snapshot or a resume puts it where it came to rest.
//!
//! What the game does (docs/spec-notes/simulation.md §5 "Dropping and taking back"):
//! a drop makes one reference per group of identical extra data, 50 units ahead of and 30
//! above the holder [observed]. That the dropped item then falls under its model's Havok
//! collision is [hypothesis]: after a scripted `player.Drop` the game reported the same
//! position for 10 s [observed], while clutter moved by `SetPos` fell and rolled
//! [observed, physics.md §4.2]; a drop from the inventory menu is not captured
//! (capture for the owner: simulation.md §5). The player's own drop from the inventory
//! menu is placed by design instead (`World::player_drop_item_at`): at chest height, each
//! new reference in the next free place of a ring around the player.

use crate::phys::cells::{PhysCell, Space};
use crate::sim::SimState;
use crate::{PlaySet, PlayWorld, RefVisual, RenderCell};
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_collide::Iso;
use fnv_phys::BodyState;
use fnv_sim::world::RefState;
use fnv_sim::FormId;
use fnv_world::transform::{mul, ref_rotation, transpose};
use fnv_world::{cell_of, WorldIndex};
use nvf_assets::model::ModelCtx;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The render cell a created reference belongs to: its interior cell, or the exterior
/// grid square containing its position in its cell's worldspace (the rule render
/// streaming assigns persistent references by). `None` when its cell is unknown.
pub fn render_cell(index: &WorldIndex, s: &RefState) -> Option<RenderCell> {
    let c = index.cells.get(&s.cell?.0)?;
    Some(match c.world {
        None => RenderCell::Interior(c.form_id),
        Some(world) => {
            let (x, y) = cell_of(s.pos[0], s.pos[1]);
            RenderCell::Exterior { world, x, y }
        }
    })
}

/// The physics cell holding a created reference while the player is in `space` of
/// worldspace `world`; `None` when it is elsewhere.
pub fn phys_cell(index: &WorldIndex, world: u32, space: Space, s: &RefState) -> Option<PhysCell> {
    match (render_cell(index, s)?, space) {
        (RenderCell::Interior(id), Space::Interior(here)) if id == here => Some(PhysCell::Interior(id)),
        (RenderCell::Exterior { world: w, x, y }, Space::Exterior) if w == world => Some(PhysCell::Exterior(x, y)),
        _ => None,
    }
}

/// Whether a created reference stands in the world as a dropped item, drawn and collided:
/// not taken or deleted, enabled, and not a shot's reference for scripts (the shot's
/// flying model is `shot_fx`'s, `RefState::shot`, #2186).
pub fn present(s: &RefState) -> bool {
    !s.deleted() && s.enabled() && !s.shot
}

/// Where a created reference is drawn (or should be).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub cell: RenderCell,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
}

impl Placed {
    /// The created references the simulation wants drawn: present, with a known cell.
    pub fn wanted<'a>(index: &WorldIndex, created: impl Iterator<Item = (FormId, &'a RefState)>) -> BTreeMap<u32, Placed> {
        created
            .filter(|(_, s)| present(s))
            .filter_map(|(id, s)| Some((id.0, Placed { cell: render_cell(index, s)?, pos: s.pos, rot: s.rot })))
            .collect()
    }

    /// Placement in game space: `p' = pos + R(rot) p` (created references have scale 1).
    pub fn affine(&self) -> Affine3A {
        let m = Mat3::from_cols_array(&ref_rotation(self.rot)).transpose(); // row-major
        Affine3A::from_mat3_translation(m, Vec3::from_array(self.pos))
    }
}

/// What to change so that the drawn created references match the simulation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// Drawn, but taken, gone from the simulation, moved to another cell, or in a cell
    /// that is no longer loaded.
    pub despawn: Vec<u32>,
    /// Wanted in a loaded cell and not drawn there (including those just despawned to
    /// move cells).
    pub spawn: Vec<u32>,
    /// Drawn in the right cell, at an old pose.
    pub moved: Vec<u32>,
}

pub fn plan(wanted: &BTreeMap<u32, Placed>, drawn: &BTreeMap<u32, Placed>, loaded: impl Fn(RenderCell) -> bool) -> Plan {
    let mut p = Plan::default();
    for (&id, d) in drawn {
        match wanted.get(&id) {
            Some(w) if w.cell == d.cell && loaded(w.cell) => {
                if w.pos != d.pos || w.rot != d.rot {
                    p.moved.push(id);
                }
            }
            _ => p.despawn.push(id),
        }
    }
    for (&id, w) in wanted {
        if loaded(w.cell) && (!drawn.contains_key(&id) || p.despawn.contains(&id)) {
            p.spawn.push(id);
        }
    }
    p
}

/// The reference pose (position, row-major rotation) of a body placed at `ref ∘ local`
/// (`local` the collision object's node and body transforms, its translation scaled by
/// the reference's `scale`), as `fnv_phys::PhysWorld::add_ref` places it: `R = R_body
/// R_localᵀ`, `t = t_body − R (scale · t_local)`.
pub fn ref_pose_from_body(body: &BodyState, local: &Iso, scale: f32) -> ([f32; 3], [f32; 9]) {
    let r = mul(&body.rotation, &transpose(&local.rotation));
    let t = local.translation.map(|v| v * scale);
    let rt = [r[0] * t[0] + r[1] * t[1] + r[2] * t[2], r[3] * t[0] + r[4] * t[1] + r[5] * t[2], r[6] * t[0] + r[7] * t[1] + r[8] * t[2]];
    let b = body.translation;
    ([b[0] - rt[0], b[1] - rt[1], b[2] - rt[2]], r)
}

/// The model a created reference draws and collides with: its base form's world model
/// (armour's MOD2 ground object, a weapon's MOD4, else MODL:
/// `BaseObject::world_model_path`), except SpeedTree models, which items never use. A
/// weapon with mods on (`mods`, the reference's `Extra::mods`) draws the third-person
/// model of its mods instead ([`modded_model`]; weapon-mods.md §3 "In the world").
pub fn model_path(d: &crate::PlayData, base: FormId, mods: u8) -> Option<String> {
    if mods != 0 {
        let w = fnv_actor::records::ActorForms::weapon_record(d.lo, d.schema, base);
        if let Some(path) = modded_model(w.as_ref(), mods) {
            return Some(path);
        }
    }
    let b = fnv_world::placed::base_object(d.lo, d.schema, base.0)?;
    b.world_model_path().filter(|m| !m.ends_with(".spt"))
}

/// The same resolved model for created drawing and collision. Session aliases keep
/// their identity, with immutable metadata from their known authored original.
pub fn created_model_path(d: &crate::PlayData, world: &fnv_sim::World<'_>, base: FormId, mods: u8) -> Option<String> {
    match world.temporary_scene_base(base) {
        Some(descriptor) => match &descriptor.model_override {
            Some(path) if path.trim().is_empty() => None,
            Some(path) => Some(fnv_actor::resolve::mesh_path(path)).filter(|p| !p.ends_with(".spt")),
            None => model_path(d, descriptor.original, mods),
        },
        None => model_path(d, base, mods),
    }
}

fn created_draws_model(d: &crate::PlayData, world: &fnv_sim::World<'_>, base: FormId, mods: u8) -> bool {
    let Some(descriptor) = world.temporary_scene_base(base) else { return draws_model(d, base, mods) };
    // A selected replacement NIF may differ from an authored SpeedTree model.
    // Type comes from the original; drawable model kind comes from the same
    // selected path physics consumes, rather than the original's stale path.
    fnv_world::placed::base_object(d.lo, d.schema, descriptor.original.0).is_some_and(|b| matches!(&b.sig, b"STAT" | b"MSTT" | b"PROJ")) && created_model_path(d, world, base, mods).is_some_and(|path| !crate::script_draw::is_marker(&path))
}

/// Whether a created reference draws the model [`model_path`] gives: the rule placed
/// references are drawn by (`crate::script_draw`), so a marker made at run time is as
/// unseen as a placed one and an explosion's reference draws no effect model. The
/// reference itself stays for scripts, and [`model_path`] stays the collision's model.
pub fn draws_model(d: &crate::PlayData, base: FormId, mods: u8) -> bool {
    fnv_world::placed::base_object(d.lo, d.schema, base.0).is_none_or(|b| crate::script_draw::drawn_type(&b)) && !model_path(d, base, mods).is_some_and(|m| crate::script_draw::is_marker(&m))
}

/// The alternate textures of the model [`model_path`] gives: the base's world-model
/// list (`BaseObject::world_alt_textures`), none for a modded weapon's own model
/// (docs/spec-notes/texture-swaps.md §4).
pub fn model_swaps(d: &crate::PlayData, base: FormId, mods: u8) -> Vec<fnv_world::alt_textures::AltTexture> {
    if mods != 0 && modded_model(fnv_actor::records::ActorForms::weapon_record(d.lo, d.schema, base).as_ref(), mods).is_some() {
        return Vec::new();
    }
    fnv_world::placed::base_object(d.lo, d.schema, base.0).map(|b| b.world_alt_textures).unwrap_or_default()
}

/// The archive path of the third-person model of weapon `w` with the mod flags `mods` on
/// (`Weapon::model_for`: the combination's `MWD` model, else the plain MODL); `None` with
/// no mods or no weapon, for the plain world model.
pub fn modded_model(w: Option<&fnv_actor::records::Weapon>, mods: u8) -> Option<String> {
    (mods & 7 != 0).then_some(())?;
    w?.model_for(false, mods).map(fnv_actor::resolve::mesh_path)
}

/// Marks the root entity of a drawn created reference (child of its cell's entity).
#[derive(Component, Clone, Copy, Debug)]
pub struct DroppedRoot(pub u32);

/// On a [`DroppedRoot`]: the model drawn (under meshes), for JIP's NIF block functions
/// (nvforever nif_mirror.rs reads the tree of an item drawn as flat shapes from it, #2132).
#[derive(Component, Clone, Debug)]
pub struct DroppedModel(pub String);

/// A drawn created reference.
#[derive(Debug, Clone, Copy)]
pub struct DrawnRef {
    pub root: Entity,
    /// The cell entity it was spawned under.
    pub cell_entity: Entity,
    pub at: Placed,
}

/// Drawn created references by FormID.
#[derive(Resource, Default)]
pub struct Drawn(pub BTreeMap<u32, DrawnRef>);

pub struct DroppedPlugin;

impl Plugin for DroppedPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Drawn>().add_systems(Update, sync_drawn.in_set(PlaySet::Sync));
    }
}

type Cells<'w, 's> = Query<'w, 's, (Entity, &'static RenderCell, &'static Transform), Without<DroppedRoot>>;

/// Bring the drawn created references in line with the simulation ([`plan`]).
#[allow(clippy::too_many_arguments)]
fn sync_drawn(
    mut commands: Commands,
    d: Res<PlayWorld>,
    sim: Res<SimState>,
    cells: Cells,
    roots: Query<(), With<DroppedRoot>>,
    mut transforms: Query<&mut Transform, With<DroppedRoot>>,
    mut drawn: ResMut<Drawn>,
    mut models: ModelCtx,
    mut not_drawn: Local<BTreeSet<(FormId, u8)>>,
) {
    let loaded: HashMap<RenderCell, (Entity, Vec3)> = cells.iter().map(|(e, c, t)| (*c, (e, t.translation))).collect();
    // Drawn under a cell entity that is gone (render streaming, door travel: the root
    // went with it) or was replaced by a new load of the same cell in one frame.
    drawn.0.retain(|id, r| {
        let kept = roots.contains(r.root) && loaded.get(&r.at.cell).is_some_and(|(e, _)| *e == r.cell_entity);
        if !kept {
            commands.entity(r.root).try_despawn();
            info!("dropped reference 0x{id:08X} no longer drawn ({:?} was unloaded)", r.at.cell);
        }
        kept
    });
    // Created actors (PlaceAtMe of an actor base) are actors, drawn and moved by the
    // engine's residents (nvforever `residents.rs`), not dropped items.
    let forms = sim.world.forms;
    let wanted = Placed::wanted(d.index, sim.world.created().filter(|(_, s)| !fnv_game::ai::is_actor_base(forms, s.base)));
    let was: BTreeMap<u32, Placed> = drawn.0.iter().map(|(&id, r)| (id, r.at)).collect();
    let p = plan(&wanted, &was, |c| loaded.contains_key(&c));
    for id in &p.despawn {
        if let Some(DrawnRef { root, at, .. }) = drawn.0.remove(id) {
            commands.entity(root).try_despawn();
            let why = if wanted.contains_key(id) { "left its cell" } else { "taken" };
            info!("dropped reference 0x{id:08X} no longer drawn ({why}; was in {:?})", at.cell);
        }
    }
    for id in p.moved {
        let w = wanted[&id];
        let Some(r) = drawn.0.get_mut(&id) else { continue };
        if let (Ok(mut t), Some((_, origin))) = (transforms.get_mut(r.root), loaded.get(&w.cell)) {
            *t = Transform::from_matrix(Mat4::from(Affine3A::from_translation(-*origin) * w.affine()));
        }
        r.at = w;
    }
    for id in p.spawn {
        let w = wanted[&id];
        let (cell_e, origin) = loaded[&w.cell];
        let Some((base, mods)) = sim.world.peek(FormId(id)).map(|s| (s.base, s.extra.mods)) else { continue };
        // Known from the records alone, so asked once: such a reference is in the plan every frame.
        if not_drawn.contains(&(base, mods)) {
            continue;
        }
        if !created_draws_model(&d, &sim.world, base, mods) {
            info!("dropped reference 0x{id:08X}: base {base} draws nothing (a marker, or a type with no model in the world)");
            not_drawn.insert((base, mods));
            continue;
        }
        let Some(path) = created_model_path(&d, &sim.world, base, mods) else {
            warn_once!("dropped reference 0x{id:08X}: base {base} has no model to draw");
            continue;
        };
        let original = sim.world.temporary_scene_base(base).map_or(base,|descriptor|descriptor.original);
        let swaps = model_swaps(&d, original, mods);
        let Some(model) = models.model_swapped(&path, false, &swaps) else {
            warn_once!("dropped reference 0x{id:08X}: model {path} could not be loaded");
            continue;
        };
        let t = Transform::from_matrix(Mat4::from(Affine3A::from_translation(-origin) * w.affine()));
        // `ChildOf` on the new entity (not a command on the cell): if render streaming
        // despawns the cell this frame, the relationship is dropped with a warning and
        // the next frame despawns the root, instead of a command failing on the cell.
        let root = commands.spawn((Name::new(format!("Dropped {id:08X}")), DroppedRoot(id), DroppedModel(path.clone()), t, Visibility::default(), ChildOf(cell_e))).id();
        // Animated: its own node hierarchy, as render streaming does, for a door, an idle,
        // or (without one) script groups. Not for an idle of nothing but parts the
        // holder's clips move (a gun's slide), which an item on the ground keeps still
        // (nvforever world.rs), nor for a rig that only plays the model as an effect
        // (`Rig::effect`, a mine's light controller): the flat shapes keep a mine's light
        // parts (`crate::mines::MineLight`).
        let shapes = match &model.rig {
            Some(rig) if rig.door.is_some() || rig.idle.as_ref().map_or(!rig.groups.is_empty(), |c| !c.only_free_motion()) => {
                let (r, drawn_shapes) = rig.spawn(&mut commands, root, Transform::IDENTITY, &path);
                commands.entity(r).insert(RefVisual(id));
                for e in &drawn_shapes {
                    commands.entity(*e).insert(RefVisual(id));
                }
                drawn_shapes.len()
            }
            _ => {
                for s in &model.shapes {
                    let e = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(s.transform)), RefVisual(id), ChildOf(root))).id();
                    if let Some(l) = crate::mines::MineLight::of(s.name.as_deref(), &s.ancestors) {
                        commands.entity(e).insert(l);
                    }
                }
                model.shapes.len()
            }
        };
        info!(
            "dropped reference 0x{id:08X} drawn: {path}, {shapes} shapes, in {:?} at ({:.1}, {:.1}, {:.1})",
            w.cell, w.pos[0], w.pos[1], w.pos[2]
        );
        drawn.0.insert(id, DrawnRef { root, cell_entity: cell_e, at: w });
    }
}

#[cfg(test)]
mod tests;
