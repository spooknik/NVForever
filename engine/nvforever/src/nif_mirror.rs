//! The drawn nodes for JIP's NIF block functions (fnv_game::nif_blocks;
//! docs/design/nif-blocks.md): each frame the scripts' node changes are applied to the
//! entities of the reference they name, and the node tree of every reference a script
//! asked about is told back. A reference's root is its actor rig (the player's: the
//! third-person body, `PlayerBody::third_rig`), told as the skeleton's root node
//! (`Scene Root` [observed: the original's head parents end there, #2132]), else the
//! topmost entity drawing it. A shape's NiMaterialProperty values (JIP's Get/SetMaterialPropertyValue)
//! are read from the reference's model file, once per model, with the scripts' changes
//! over them; a change draws the shape with its own copy of the material. The vertices of
//! the shapes a script asked for (SUP's NifBlockGetVerticles) are told back each frame.
//! A created item drawn as flat shapes (`nvf_play::dropped`, a weapon on the ground) has
//! no node entities: its tree is told from its model file (names, parents, local
//! transforms), with the scripts' set transforms over it (#2132).

use crate::limbs::ActorRig;
use bevy::prelude::*;
use bevy::camera::primitives::Aabb;
use fnv_game::nif_blocks::{NodeInfo, SceneEdit, SceneNode};
use fnv_sim::FormId;
use nvf_assets::{GameData, NifHidden, NifMaterial};
use nvf_play::script::ScriptState;
use nvf_play::RefVisual;
use std::collections::HashMap;

pub struct NifMirrorPlugin;

impl Plugin for NifMirrorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, mirror.before(TransformSystems::Propagate));
    }
}

/// What the mirror keeps between frames: the references it logged, the held transforms,
/// the models' material values, the scripts' material values and the shapes drawn with
/// their own material since.
#[derive(Default)]
struct Memory {
    logged: std::collections::HashSet<u32>,
    held: HashMap<(u32, String), Held>,
    tables: HashMap<String, HashMap<String, [f32; 9]>>,
    materials: HashMap<(u32, String), [f32; 9]>,
    owned: HashMap<Entity, Handle<NifMaterial>>,
    /// Each model's shapes' NiAlphaProperty and NiStencilProperty values, by shape name.
    props: HashMap<String, HashMap<String, Props>>,
    /// Each model's node tree as its file holds it, for items drawn as flat shapes.
    files: HashMap<String, Vec<FileNode>>,
}

/// A node of a model file: its name, its parent's index, its local transform.
#[derive(Clone)]
struct FileNode {
    name: String,
    parent: Option<usize>,
    translation: Vec3,
    rotation: Quat,
    scale: f32,
}

/// A model file's nodes and shapes (under meshes), depth first from its root, children in
/// stored order, as `walk` tells drawn ones.
fn file_tree(data: &GameData, model: &str) -> Vec<FileNode> {
    let mut out = Vec::new();
    if model.is_empty() {
        return out;
    }
    let Some(bytes) = data.read(&format!("meshes\\{model}")) else { return out };
    let Ok(file) = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)) else { return out };
    let dec = fnv_nif::scene::Decoded::new(&file, &bytes, &data.schema);
    let objects = fnv_nif::scene::scene_objects(&file, &dec);
    let by_block: HashMap<usize, &fnv_nif::scene::SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
    let mut stack: Vec<(usize, Option<usize>)> = objects.iter().filter(|o| o.parent.is_none()).map(|o| (o.block, None)).rev().collect();
    let mut seen = std::collections::HashSet::new();
    while let Some((b, parent)) = stack.pop() {
        let Some(o) = by_block.get(&b).filter(|_| seen.insert(b)) else { continue };
        let t = &o.transform;
        // The stored rows taken as columns: the angles the original reads of a file's node
        // [observed: GetNifBlockRotation of the 10mm pistol's ProjectileNode, 90 0 90, is
        // this matrix's; the transpose nvf_assets::nif_spawn draws with gives its inverse].
        let m = Mat3::from_cols_array(&t.rotation);
        let at = out.len();
        out.push(FileNode { name: o.name.clone().unwrap_or_default(), parent, translation: Vec3::from_array(t.translation), rotation: Quat::from_mat3(&m), scale: t.scale });
        stack.extend(o.children.iter().rev().map(|&c| (c, Some(at))));
    }
    out
}

/// A shape's alpha (flags, threshold) and stencil (flags, reference, mask) properties.
type Props = (Option<[f32; 2]>, Option<[f32; 3]>);

/// Each named shape's NiAlphaProperty and NiStencilProperty values in a model file (under
/// meshes), for JohnnyGuitar's Get…PropertyValue [documented: nif.xml's fields;
/// hypothesis: the functions' item order is the fields'].
fn property_table(data: &GameData, model: &str) -> HashMap<String, Props> {
    let mut out: HashMap<String, Props> = HashMap::new();
    if model.is_empty() {
        return out;
    }
    let Some(bytes) = data.read(&format!("meshes\\{model}")) else { return out };
    let Ok(file) = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)) else { return out };
    let dec = fnv_nif::scene::Decoded::new(&file, &bytes, &data.schema);
    for i in 0..file.blocks.len() {
        let Some(s) = dec.get(i as i32) else { continue };
        let Some(name) = s.text("name").filter(|n| !n.is_empty()) else { continue };
        for &r in s.refs("properties").unwrap_or_default() {
            let (Some(b), Some(p)) = (file.blocks.get(r as usize), dec.get(r)) else { continue };
            let int = |k: &str| p.int(k).map(|v| v as f32);
            let e = out.entry(name.to_string()).or_default();
            match b.type_name.as_str() {
                "NiAlphaProperty" => e.0 = Some([int("flags").unwrap_or(0.0), int("threshold").unwrap_or(0.0)]),
                "NiStencilProperty" => {
                    let reference = int("stencil_ref").or_else(|| int("stencil_reference")).unwrap_or(0.0);
                    let mask = int("stencil_mask").unwrap_or(0.0);
                    e.1 = Some([int("flags").unwrap_or(0.0), reference, mask]);
                }
                _ => {}
            }
        }
    }
    out
}

/// The world-space bounding sphere of `node` and what lies under it: of its shapes'
/// boxes, or the node's own place (radius 0) without any.
fn node_bound(at: usize, tree: &[(Entity, Option<usize>)], globals: &Query<&GlobalTransform>, boxes: &Query<&Aabb>) -> Option<[f32; 4]> {
    let mut spheres: Vec<(Vec3, f32)> = Vec::new();
    for (k, (e, _)) in tree.iter().enumerate() {
        // k under `at` (or `at` itself).
        let mut up = Some(k);
        let mut under = false;
        while let Some(i) = up {
            if i == at {
                under = true;
                break;
            }
            up = tree[i].1;
        }
        if !under {
            continue;
        }
        if let (Ok(b), Ok(g)) = (boxes.get(*e), globals.get(*e)) {
            let c = g.transform_point(Vec3::from(b.center));
            let s = g.compute_transform().scale.abs().max_element();
            spheres.push((c, Vec3::from(b.half_extents).length() * s));
        }
    }
    let own = globals.get(tree.get(at)?.0).ok()?.translation();
    let (centre, radius) = if spheres.is_empty() {
        (own, 0.0)
    } else {
        let (lo, hi) = spheres.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), (c, r)| (lo.min(*c - *r), hi.max(*c + *r)));
        let mid = (lo + hi) / 2.0;
        (mid, spheres.iter().map(|(c, r)| c.distance(mid) + r).fold(0.0, f32::max))
    };
    let g = nvf_play::space::to_game(centre);
    Some([g[0], g[1], g[2], radius])
}

/// What the mirror reads beside the nodes: their places in the world and boxes (bounds),
/// and the particle systems.
type Extra<'w, 's> = (Query<'w, 's, &'static GlobalTransform>, Query<'w, 's, &'static Aabb>, Option<ResMut<'w, crate::particles::Particles>>, Option<Res<'w, crate::world::WorldData>>);

/// Each named shape's NiMaterialProperty values in a model file (under meshes), in
/// GetMaterialPropertyValue's order: specular R G B, emissive R G B, glossiness, alpha,
/// emissive mult [documented: the page's value IDs; the fields: nif.xml
/// NiMaterialProperty]. The emissive mult is 1 in files without it.
fn material_table(data: &GameData, model: &str) -> HashMap<String, [f32; 9]> {
    let mut out = HashMap::new();
    if model.is_empty() {
        return out;
    }
    let Some(bytes) = data.read(&format!("meshes\\{model}")) else { return out };
    let Ok(file) = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)) else { return out };
    let dec = fnv_nif::scene::Decoded::new(&file, &bytes, &data.schema);
    for i in 0..file.blocks.len() {
        let Some(s) = dec.get(i as i32) else { continue };
        let Some(name) = s.text("name").filter(|n| !n.is_empty()) else { continue };
        for &r in s.refs("properties").unwrap_or_default() {
            if !file.blocks.get(r as usize).is_some_and(|b| b.type_name == "NiMaterialProperty") {
                continue;
            }
            let Some(m) = dec.get(r) else { continue };
            let c = |f: &str| m.floats(f).filter(|v| v.len() >= 3).map_or([0.0; 3], |v| [v[0], v[1], v[2]]);
            let (sp, em) = (c("specular_color"), c("emissive_color"));
            let v = [sp[0], sp[1], sp[2], em[0], em[1], em[2], m.float("glossiness").unwrap_or(0.0), m.float("alpha").unwrap_or(1.0), m.float("emissive_mult").unwrap_or(1.0)];
            out.entry(name.to_string()).or_insert(v);
        }
    }
    out
}

/// A shape's material drawn with these values: the specular tint, the emitted light
/// (colour times mult), the roughness of the glossiness, the alpha [hypothesis: the
/// spawn's own display tuning of the emitted light is not redone here].
fn set_material(m: &mut NifMaterial, v: &[f32; 9]) {
    m.base.specular_tint = Color::linear_rgb(v[0], v[1], v[2]);
    m.base.emissive = LinearRgba::rgb(v[3] * v[8], v[4] * v[8], v[5] * v[8]);
    m.base.perceptual_roughness = nvf_assets::nif_spawn::gloss_roughness(v[6]);
    m.base.base_color.set_alpha(v[7].clamp(0.0, 1.0));
}

/// A node transform a script set, held over the animation.
#[derive(Clone, Copy, Default)]
struct Held {
    translation: Option<Vec3>,
    rotation: Option<Quat>,
    scale: Option<f32>,
}

type Visuals<'w, 's> = Query<'w, 's, (Entity, &'static RefVisual, Option<&'static ChildOf>), crate::lights::NotLightSource>;

type NodeQuery<'w, 's> = Query<'w, 's, (Option<&'static Name>, &'static mut Transform, Option<&'static Children>, Has<NifHidden>)>;

/// The actors' rigs: the placed actors' and the player's third-person body.
type Rigs<'w, 's> = (Query<'w, 's, (Entity, &'static ActorRig)>, Option<Res<'w, crate::player_body::PlayerBody>>);

/// A reference's rig: its root entity and the rig.
fn rig_of(r: u32, (actors, player): &Rigs) -> Option<(Entity, ActorRig)> {
    if r == FormId::PLAYER_REF.0 {
        return player.as_deref().and_then(crate::player_body::PlayerBody::third_rig);
    }
    actors.iter().find(|(_, a)| a.reference == r).map(|(e, a)| (e, a.clone()))
}

/// The root entity of a reference's drawing.
fn root_of(r: u32, rigs: &Rigs, visuals: &Visuals) -> Option<Entity> {
    if let Some((e, _)) = rig_of(r, rigs) {
        return Some(e);
    }
    visuals.iter().filter(|(_, v, _)| v.0 == r).find(|(_, _, p)| p.is_none_or(|p| visuals.get(p.parent()).map_or(true, |(_, v, _)| v.0 != r))).map(|(e, _, _)| e)
}

/// The node entities under `root`, depth first, with their parent's index; an actor's
/// root first (its skeleton's root node), the rest under it.
fn walk(root: Entity, nodes: &NodeQuery, rig: &RigNames) -> Vec<(Entity, Option<usize>)> {
    let mut out = Vec::new();
    let mut stack: Vec<(Entity, Option<usize>)> = Vec::new();
    let top = if rig.root == Some(root) {
        out.push((root, None));
        Some(0)
    } else {
        None
    };
    if let Ok((_, _, Some(ch), _)) = nodes.get(root) {
        stack.extend(ch.iter().rev().map(|c| (c, top)));
    }
    while let Some((e, parent)) = stack.pop() {
        let at = out.len();
        out.push((e, parent));
        if let Ok((_, _, Some(ch), _)) = nodes.get(e) {
            stack.extend(ch.iter().rev().map(|c| (c, Some(at))));
        }
    }
    out
}

/// An actor's skeleton node names, by entity (its node entities carry no `Name`), and its
/// root entity (named by the skeleton, not by the `Name` the spawn gives it).
#[derive(Default)]
struct RigNames {
    root: Option<Entity>,
    names: HashMap<Entity, String>,
}

fn rig_names(r: u32, rigs: &Rigs) -> RigNames {
    rig_of(r, rigs)
        .map(|(root, a)| RigNames { root: Some(root), names: a.nodes.iter().zip(&a.skel.names).filter_map(|(e, n)| n.clone().map(|n| (*e, n))).collect() })
        .unwrap_or_default()
}

/// A node's name: an actor root's from its skeleton, else its `Name`, else the rig's.
fn name_of(e: Entity, nodes: &NodeQuery, rig: &RigNames) -> String {
    if rig.root == Some(e) {
        if let Some(n) = rig.names.get(&e) {
            return n.clone();
        }
    }
    nodes.get(e).ok().and_then(|n| n.0.map(|n| n.as_str().to_string())).or_else(|| rig.names.get(&e).cloned()).unwrap_or_default()
}

/// The node of that name under `root`: exactly, else without regard to case.
fn find(root: Entity, name: &str, nodes: &NodeQuery, rig: &RigNames) -> Option<Entity> {
    let all = walk(root, nodes, rig);
    all.iter().map(|(e, _)| *e).find(|&e| name_of(e, nodes, rig) == name).or_else(|| all.iter().map(|(e, _)| *e).find(|&e| name_of(e, nodes, rig).eq_ignore_ascii_case(name)))
}

/// The material side of [`mirror`]: the data files, the shapes' materials, the assets.
type Materials<'w, 's> = (Option<Res<'w, GameData>>, Query<'w, 's, &'static MeshMaterial3d<NifMaterial>>, Option<ResMut<'w, Assets<NifMaterial>>>);

/// The roots of created items (`nvf_play::dropped`) and the entities with children: an
/// item whose shapes hang straight from its root is drawn flat.
type Dropped<'w, 's> = (Query<'w, 's, (&'static nvf_play::dropped::DroppedRoot, Option<&'static nvf_play::dropped::DroppedModel>, Option<&'static Children>)>, Query<'w, 's, (), With<Children>>);

/// The model of `r` when it is a created item drawn as flat shapes (no node entities).
fn drawn_flat(r: u32, (roots, nested): &Dropped) -> Option<String> {
    roots.iter().find(|(d, _, ch)| d.0 == r && ch.is_some_and(|c| c.iter().all(|e| !nested.contains(e)))).map(|(_, m, _)| {
        // The drop's path is under meshes already; the tables here take it without.
        let m = m.map(|m| m.0.as_str()).unwrap_or_default();
        m.get(..7).filter(|p| p.eq_ignore_ascii_case("meshes\\")).map_or(m, |_| &m[7..]).to_string()
    })
}

/// The shapes' meshes, for SUP's NifBlockGetVerticles.
type Shapes<'w, 's> = (Query<'w, 's, &'static Mesh3d>, Option<Res<'w, Assets<Mesh>>>);

#[allow(clippy::too_many_arguments)]
fn mirror(
    mut script: Option<ResMut<ScriptState>>,
    rigs: Rigs,
    visuals: Visuals,
    parents: Query<&ChildOf>,
    mut nodes: NodeQuery,
    mut commands: Commands,
    mut mem: Local<Memory>,
    (data, drawn, mut assets): Materials,
    (shapes, meshes): Shapes,
    (globals, boxes, mut particles, world): Extra,
    dropped: Dropped,
) {
    let Memory { logged, held, tables, materials, owned, props, files } = &mut *mem;
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let edits = rt.game.scene.take_edits();
    let mut edits_rest = Vec::new();
    for edit in edits {
        // A transform a script sets holds over the animation, frame after frame
        // [observed: set-then-read stays exact on an animated bone]; kept for a model
        // drawn flat too, whose tree is its file's.
        match edit {
            SceneEdit::Translation { r, node, v } => {
                held.entry((r.0, node)).or_default().translation = Some(Vec3::from(v));
                continue;
            }
            SceneEdit::Rotation { r, node, v } => {
                let [x, y, z] = v.map(f32::to_radians);
                held.entry((r.0, node)).or_default().rotation = Some(Quat::from_euler(EulerRot::ZYX, z, y, x));
                continue;
            }
            SceneEdit::Scale { r, node, v } => {
                held.entry((r.0, node)).or_default().scale = Some(v);
                continue;
            }
            edit => edits_rest.push(edit),
        }
    }
    for edit in edits_rest {
        let r = match &edit {
            SceneEdit::Translation { r, .. } | SceneEdit::Rotation { r, .. } | SceneEdit::Scale { r, .. } | SceneEdit::Flags { r, .. } | SceneEdit::ChildFlags { r, .. } | SceneEdit::Rename { r, .. } | SceneEdit::Remove { r, .. } | SceneEdit::Insert { r, .. } | SceneEdit::Attach { r, .. } | SceneEdit::Light { r, .. } | SceneEdit::Material { r, .. } | SceneEdit::Particle { r, .. } => *r,
        };
        let Some(root) = root_of(r.0, &rigs, &visuals) else { continue };
        let rig = rig_names(r.0, &rigs);
        match edit {
            SceneEdit::Translation { .. } | SceneEdit::Rotation { .. } | SceneEdit::Scale { .. } => {}
            SceneEdit::Flags { node, flags, .. } => {
                if let Some(e) = find(root, &node, &nodes, &rig) {
                    if flags & 1 != 0 {
                        commands.entity(e).insert((NifHidden, Visibility::Hidden));
                    } else {
                        commands.entity(e).remove::<NifHidden>().insert(Visibility::Inherited);
                    }
                }
            }
            // A child by its place under the node, in the order `walk` told the mirror
            // (a switch's children, #2017).
            SceneEdit::ChildFlags { node, child, flags, .. } => {
                let kid = find(root, &node, &nodes, &rig).and_then(|p| nodes.get(p).ok().and_then(|(_, _, ch, _)| ch.and_then(|c| c.iter().nth(child))));
                if let Some(e) = kid {
                    if flags & 1 != 0 {
                        commands.entity(e).insert((NifHidden, Visibility::Hidden));
                    } else {
                        commands.entity(e).remove::<NifHidden>().insert(Visibility::Inherited);
                    }
                }
            }
            SceneEdit::Rename { node, to, .. } => {
                if let Some(e) = find(root, &node, &nodes, &rig) {
                    commands.entity(e).insert(Name::new(to.clone()));
                    if let Some(h) = held.remove(&(r.0, node)) {
                        held.insert((r.0, to), h);
                    }
                }
            }
            SceneEdit::Remove { node, .. } => {
                if let Some(e) = find(root, &node, &nodes, &rig) {
                    commands.entity(e).despawn();
                }
                held.remove(&(r.0, node));
            }
            SceneEdit::Insert { target, name, as_parent, .. } => {
                let Some(t) = find(root, &target, &nodes, &rig) else { continue };
                let new = commands.spawn((Name::new(name), Transform::IDENTITY, Visibility::Inherited)).id();
                if as_parent {
                    let above = parents.get(t).map(|p| p.parent()).unwrap_or(root);
                    commands.entity(above).add_child(new);
                    commands.entity(new).add_child(t);
                } else {
                    commands.entity(t).add_child(new);
                }
            }
            // The model is not drawn yet [hypothesis: slice 2 spawns it through
            // nvf_assets::SpawnCtx]; the mirror holds the request only.
            SceneEdit::Attach { .. } => {}
            // JIP's AttachLight: a node of that name under the target at the offset, lit
            // by the LIGH's colour, brightness and radius as a muzzle flash's light is
            // (`crate::shot_fx`; docs/spec-notes/ext-unlisted-commands.md §2.1).
            SceneEdit::Light { target, name, light, offset, .. } => {
                let Some(t) = find(root, &target, &nodes, &rig) else { continue };
                let data = world.as_deref().and_then(|w| fnv_world::lighting::light_data(w.lo, w.schema, light.0));
                let new = commands.spawn((Name::new(name), Transform::from_translation(Vec3::from(offset)), Visibility::Inherited)).id();
                if let Some(l) = data.filter(|l| l.radius > 0.0) {
                    let color = crate::lights::display_color(l.color.map(|b| b as f32 / 255.0));
                    let intensity = crate::lights::lumens(l.fade.max(0.0), crate::sky::light_unit());
                    commands.entity(new).insert(PointLight { color, intensity, range: l.radius, shadow_maps_enabled: false, ..default() });
                    info!("nif mirror: light {:08X} on {:08X}'s {target}, radius {:.0}", light.0, r.0, l.radius);
                }
                commands.entity(t).add_child(new);
            }
            // A placed model's particle system of that name (or emitting from a node of
            // that name), changed while it lives (JohnnyGuitar's SetNiPSysModifierValue).
            SceneEdit::Particle { node, id, v, .. } => {
                if let Some(p) = particles.as_deref_mut() {
                    for d in crate::particles::placed_systems(p, r.0) {
                        if d.name.eq_ignore_ascii_case(&node) || d.emitter.object.as_deref().is_some_and(|o| o.eq_ignore_ascii_case(&node)) {
                            crate::particles::set_script_value(d, id, v);
                        }
                    }
                }
            }
            // The mirror has the value already; the shape is drawn with its own copy of
            // the material from now on [hypothesis: the original's change is this
            // reference's; the page's note puts an inventory reference's on the base].
            SceneEdit::Material { node, id, v, .. } => {
                let Some(d) = data.as_deref() else { continue };
                let model = fnv_game::nif_blocks::model_of(&rt.game, r).unwrap_or_default();
                let table = tables.entry(model.clone()).or_insert_with(|| material_table(d, &model));
                let base = node.split(':').next().unwrap_or(&node).to_string();
                let Some(&start) = table.get(&node).or_else(|| table.get(&base)) else { continue };
                let vals = materials.entry((r.0, node.clone())).or_insert(start);
                vals[id] = v;
                let vals = *vals;
                let (Some(e), Some(assets)) = (find(root, &node, &nodes, &rig), assets.as_deref_mut()) else { continue };
                let Ok(handle) = drawn.get(e) else { continue };
                match owned.get(&e).filter(|h| *h == &handle.0).cloned() {
                    Some(h) => {
                        if let Some(mut m) = assets.get_mut(&h) {
                            set_material(&mut m, &vals);
                        }
                    }
                    None => {
                        let Some(mut m) = assets.get(&handle.0).cloned() else { continue };
                        set_material(&mut m, &vals);
                        let h = assets.add(m);
                        owned.insert(e, h.clone());
                        commands.entity(e).insert(MeshMaterial3d(h));
                    }
                }
            }
        }
    }
    // The held transforms, over this frame's pose.
    let keys: Vec<(u32, String)> = held.keys().cloned().collect();
    for (r, node) in keys {
        let Some(root) = root_of(r, &rigs, &visuals) else { continue };
        let rig = rig_names(r, &rigs);
        let h = held[&(r, node.clone())];
        if let Some(mut t) = find(root, &node, &nodes, &rig).and_then(|e| nodes.get_mut(e).ok()) {
            if let Some(v) = h.translation {
                t.1.translation = v;
            }
            if let Some(q) = h.rotation {
                t.1.rotation = q;
            }
            if let Some(s) = h.scale {
                t.1.scale = Vec3::splat(s);
            }
        }
    }
    // The trees of the references scripts asked about.
    // Once scripts use the NIF functions, the player's tree is told too, watched or not:
    // the original has the player's 3D at once, so a script's first read finds it
    // [observed: player.GetChildBlocks in the nif-blocks set, #2132].
    let mut watched: Vec<FormId> = rt.game.scene.watched().collect();
    // So are created items drawn flat, whose 3D the original has a frame after
    // PlaceAtMe (the nif-blocks set's first read of the pistol finds it).
    if !watched.is_empty() {
        for r in std::iter::once(FormId::PLAYER_REF).chain(dropped.0.iter().map(|(d, _, _)| FormId(d.0))) {
            if !watched.contains(&r) {
                watched.push(r);
            }
        }
    }
    for r in watched {
        if let Some(model) = drawn_flat(r.0, &dropped) {
            let Some(d) = data.as_deref() else { continue };
            let file = files.entry(model.clone()).or_insert_with(|| file_tree(d, &model));
            let table = tables.entry(model.clone()).or_insert_with(|| material_table(d, &model));
            let mut seen: HashMap<String, u32> = HashMap::new();
            let tree: Vec<SceneNode> = file
                .iter()
                .map(|f| {
                    let n = seen.entry(f.name.to_ascii_lowercase()).or_insert(0);
                    let shown = if *n == 0 { f.name.clone() } else { format!("{}:{}", f.name, *n - 1) };
                    *n += 1;
                    let h = held.get(&(r.0, shown.clone())).copied().unwrap_or_default();
                    let (z, y, x) = h.rotation.unwrap_or(f.rotation).to_euler(EulerRot::ZYX);
                    let material = materials.get(&(r.0, shown.clone())).or_else(|| table.get(&f.name)).copied();
                    SceneNode {
                        name: shown,
                        parent: f.parent,
                        translation: h.translation.unwrap_or(f.translation).to_array(),
                        rotation: [x.to_degrees(), y.to_degrees(), z.to_degrees()],
                        scale: h.scale.unwrap_or(f.scale),
                        flags: 0,
                        material,
                        info: None,
                    }
                })
                .collect();
            if logged.insert(r.0 ^ 0x4000_0000) {
                info!("nif mirror: {:08X} told from its model file {model:?} (drawn flat), {} nodes, first {:?}", r.0, tree.len(), tree.iter().take(4).map(|n| n.name.as_str()).collect::<Vec<_>>());
            }
            rt.game.scene.set_tree(r, tree);
            continue;
        }
        let Some(root) = root_of(r.0, &rigs, &visuals) else {
            if logged.insert(r.0) {
                info!("nif mirror: {:08X} watched, not drawn", r.0);
            }
            rt.game.scene.forget(r);
            continue;
        };
        let rig = rig_names(r.0, &rigs);
        let mut seen: HashMap<String, u32> = HashMap::new();
        let model = fnv_game::nif_blocks::model_of(&rt.game, r).unwrap_or_default();
        let table = match data.as_deref() {
            Some(d) => tables.entry(model.clone()).or_insert_with(|| material_table(d, &model)).clone(),
            None => HashMap::new(),
        };
        let props = match data.as_deref() {
            Some(d) => props.entry(model.clone()).or_insert_with(|| property_table(d, &model)).clone(),
            None => HashMap::new(),
        };
        let systems: Vec<(String, Option<String>, [f32; 21])> = particles
            .as_deref_mut()
            .map(|p| crate::particles::placed_systems(p, r.0).into_iter().map(|d| (d.name.clone(), d.emitter.object.clone(), crate::particles::script_values(d))).collect())
            .unwrap_or_default();
        let walked = walk(root, &nodes, &rig);
        let tree: Vec<SceneNode> = walked
            .iter()
            .enumerate()
            .map(|(k, &(e, parent))| (k, e, parent))
            .filter_map(|(k, e, parent)| {
                let base = name_of(e, &nodes, &rig);
                let (_, t, _, hidden) = nodes.get(e).ok()?;
                // A name seen before gets ":0", ":1" … [observed: "Object", "Object:0"].
                let n = seen.entry(base.to_ascii_lowercase()).or_insert(0);
                let shown = if *n == 0 { base.clone() } else { format!("{base}:{}", *n - 1) };
                *n += 1;
                let (z, y, x) = t.rotation.to_euler(EulerRot::ZYX);
                let material = materials.get(&(r.0, shown.clone())).or_else(|| table.get(&base)).copied();
                let (alpha, stencil) = props.get(&base).copied().unwrap_or_default();
                let emitter = systems.iter().find(|(n, o, _)| n.eq_ignore_ascii_case(&base) || o.as_deref().is_some_and(|o| o.eq_ignore_ascii_case(&base))).map(|s| s.2);
                let info = NodeInfo { bound: node_bound(k, &walked, &globals, &boxes), emitter, alpha, stencil };
                Some(SceneNode {
                    name: shown,
                    parent,
                    translation: t.translation.to_array(),
                    rotation: [x.to_degrees(), y.to_degrees(), z.to_degrees()],
                    scale: t.scale.x,
                    flags: u32::from(hidden),
                    material,
                    info: Some(Box::new(info)),
                })
            })
            .collect();
        if logged.insert(r.0 | 0x8000_0000) {
            info!("nif mirror: {:08X} told, {} nodes, first {:?}", r.0, tree.len(), tree.iter().take(4).map(|n| n.name.as_str()).collect::<Vec<_>>());
        }
        rt.game.scene.set_tree(r, tree);
    }
    // The vertices of the shapes scripts asked about (SUP's NifBlockGetVerticles), in the
    // shape's own space as its mesh holds them; none for a mesh kept on the GPU only
    // [hypothesis: a skinned shape's are its bind pose].
    let wanted: Vec<(FormId, String)> = rt.game.scene.wanted_vertices().cloned().collect();
    for (r, node) in wanted {
        let verts = root_of(r.0, &rigs, &visuals)
            .and_then(|root| find(root, &node, &nodes, &rig_names(r.0, &rigs)))
            .and_then(|e| shapes.get(e).ok())
            .and_then(|m| meshes.as_deref()?.get(&m.0))
            // `try_`: a mesh extracted to the render world gives none, where `attribute` panics (#2202).
            .and_then(|mesh| match mesh.try_attribute_option(Mesh::ATTRIBUTE_POSITION).ok()?? {
                bevy::mesh::VertexAttributeValues::Float32x3(v) => Some(v.clone()),
                _ => None,
            });
        rt.game.scene.set_vertices(r, &node, verts);
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn a_light_source_is_not_a_references_root() {
        const R: u32 = 0x301;
        fn root(world: &mut World) -> Option<Entity> {
            world.run_system_once(|rigs: Rigs, visuals: Visuals| root_of(R, &rigs, &visuals)).unwrap()
        }
        // Without a parent, so that each entity passes the test on its parent and only the
        // filter on light sources can leave it out.
        let mut world = World::new();
        world.spawn((PointLight::default(), RefVisual(R)));
        world.spawn((SpotLight::default(), RefVisual(R)));
        assert_eq!(root(&mut world), None, "its light sources alone");
        let drawn = world.spawn((Transform::IDENTITY, RefVisual(R))).id();
        assert_eq!(root(&mut world), Some(drawn), "not a light source beside it");
    }
}
