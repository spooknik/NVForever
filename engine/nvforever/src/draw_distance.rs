//! The draw-distance sliders of the settings page (Display; owner request 2026-10-16):
//! how close to the camera objects, actors and items must be to be drawn, how far the
//! sun's shadows reach and how far grass grows, each as a part of its full distance. At
//! 100% each draws as before the sliders; 50% draws to half the distance, and so on.
//!
//! The full distances [design]:
//! - **Objects, actors, items**: nothing culled them before but their cells, so the full
//!   distance is the farthest a loaded cell reaches, `(--radius + 1)` cells along both
//!   axes from the camera's cell (the diagonal). A mesh of a placed reference is drawn
//!   while its origin is within the slider's part of that, and dithers out over the
//!   [`FADE`] part beyond it. Actors are the NPCs and creatures (their bodies, worn and
//!   held items); items are the references whose base is an item a player can take
//!   (weapons, armour, ammo, aid, books, notes, keys, mods, chips, caps, cards) and the
//!   items dropped in the game; objects are every other placed model of a loaded cell.
//!   Imposters, distant LOD, terrain, water and trees are not touched.
//! - **Shadows**: `--shadow-distance` (the sun's shadow cascades end at the slider's part
//!   of it).
//! - **Grass**: `--grass-distance` and `--grass-fade-range` together.
//!
//! How it culls: Bevy's visibility range, which cell fades (`fade.rs`) and door fades
//! (`door_fade.rs`) already give every streamed mesh. A mesh at rest (fully faded in)
//! gets the range [`fade::drawn_within`] its class's distance instead of the open-ended
//! one; one still fading is left to its fade and gets its range when the fade ends. A
//! mesh beyond its range is culled on the CPU too, and casts no shadow then. At 100% the
//! range is the open-ended one again, so nothing changes against the sliders' absence.
//!
//! The sliders are kept in `NVForever.ini` [Settings] like the page's switches
//! (`nvf_play::settings`).

use crate::fade;
use crate::grass::GrassConfig;
use crate::limbs::ActorRig;
use crate::sky::SkyState;
use crate::world::{CellEntity, WorldData};
use crate::Args;
use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;
use fnv_world::placed::{base_object, placement};
use fnv_world::CELL_SIZE;
use nvf_assets::NifMaterial;
use nvf_play::dropped::DroppedRoot;
use nvf_play::RefVisual;
use std::collections::HashMap;

/// The part of a mesh's draw distance over which it dithers out beyond it [design].
pub const FADE: f32 = 0.1;

/// The sliders' least value and step.
pub const MIN: f32 = 0.1;
pub const STEP: f32 = 0.05;

/// The base object types that count as items [documented: the item records, as
/// `nvf_play::world_sounds`' pick-up sounds; MISC is the clutter a player can take].
const ITEM_SIGS: [&[u8; 4]; 12] = [b"MISC", b"KEYM", b"WEAP", b"ARMO", b"AMMO", b"ALCH", b"BOOK", b"NOTE", b"IMOD", b"CHIP", b"CCRD", b"CMNY"];

/// What a reference's mesh counts as for the sliders.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DrawClass {
    Object,
    Actor,
    Item,
    /// Not under a loaded cell (an imposter): drawn as before.
    Unranged,
}

/// The sliders' values (each a part of its full distance, [`MIN`] to 1) and the full
/// distances (module notes).
#[derive(Resource, Clone, Debug)]
pub struct DrawDistances {
    pub objects: f32,
    pub actors: f32,
    pub items: f32,
    pub shadows: f32,
    pub grass: f32,
    /// The farthest a loaded cell reaches from the camera (game units).
    pub reach: f32,
    /// `--shadow-distance`.
    pub shadow_full: f32,
    /// `--grass-distance` and `--grass-fade-range`.
    pub grass_full: (f32, f32),
}

impl DrawDistances {
    pub fn new(args: &Args) -> DrawDistances {
        DrawDistances {
            objects: 1.0,
            actors: 1.0,
            items: 1.0,
            shadows: 1.0,
            grass: 1.0,
            reach: full_reach(args.radius),
            shadow_full: args.shadow_distance.max(0.0),
            grass_full: (args.grass_distance.max(0.0), args.grass_fade_range.max(0.0)),
        }
    }

    /// The slider's part for a class (1 for one not ranged).
    pub fn part(&self, class: DrawClass) -> f32 {
        match class {
            DrawClass::Object => self.objects,
            DrawClass::Actor => self.actors,
            DrawClass::Item => self.items,
            DrawClass::Unranged => 1.0,
        }
    }

    /// How far a class is drawn; `None`: as far as its cells (the full slider).
    pub fn distance(&self, class: DrawClass) -> Option<f32> {
        let part = self.part(class);
        (part < 1.0).then_some(self.reach * part)
    }
}

/// The farthest a loaded cell reaches from the camera with `radius` cells loaded round
/// its cell: `radius + 1` cells along both axes.
pub fn full_reach(radius: i32) -> f32 {
    (radius.max(0) + 1) as f32 * CELL_SIZE * std::f32::consts::SQRT_2
}

pub struct DrawDistancePlugin(pub DrawDistances);

impl Plugin for DrawDistancePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone())
            .insert_resource(nvf_play::perk_marks::ActorDrawDistance(self.0.distance(DrawClass::Actor)))
            .add_systems(Update, (apply_ranges.after(fade::fade_cells), share_actor_distance));
    }
}

/// How far actors are drawn, for Spotter (`nvf_play::perk_marks`): the Actors slider's
/// distance, or as far as their cells at 100%.
fn share_actor_distance(distances: Res<DrawDistances>, mut shared: ResMut<nvf_play::perk_marks::ActorDrawDistance>) {
    if distances.is_changed() {
        shared.set_if_neq(nvf_play::perk_marks::ActorDrawDistance(distances.distance(DrawClass::Actor)));
    }
}

/// The class of a mesh: from its ancestors (an actor's rig, a dropped item, the cell it
/// is in) and the reference's records, kept per reference.
fn classify(e: Entity, r: u32, parents: &Query<&ChildOf>, marks: &Query<(Has<CellEntity>, Has<ActorRig>, Has<DroppedRoot>)>, world: Option<&WorldData>, known: &mut HashMap<u32, DrawClass>) -> DrawClass {
    let mut in_cell = false;
    let mut at = e;
    loop {
        if let Ok((cell, actor, dropped)) = marks.get(at) {
            if actor {
                return DrawClass::Actor;
            }
            if dropped {
                return DrawClass::Item;
            }
            if cell {
                in_cell = true;
                break;
            }
        }
        match parents.get(at) {
            Ok(p) => at = p.parent(),
            Err(_) => break,
        }
    }
    if !in_cell {
        return DrawClass::Unranged;
    }
    *known.entry(r).or_insert_with(|| {
        let Some(w) = world else { return DrawClass::Object };
        let Some(p) = placement(w.lo, w.schema, r) else { return DrawClass::Object };
        if matches!(&p.sig, b"ACHR" | b"ACRE") {
            return DrawClass::Actor;
        }
        match p.base.and_then(|b| base_object(w.lo, w.schema, b)) {
            Some(b) if ITEM_SIGS.contains(&&b.sig) => DrawClass::Item,
            _ => DrawClass::Object,
        }
    })
}

/// Give the meshes of placed references the range of their class's distance: new meshes
/// and those whose range changed (a fade ended) each frame, every one when a slider
/// moved (module notes).
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn apply_ranges(
    mut commands: Commands,
    distances: Res<DrawDistances>,
    world: Option<Res<WorldData>>,
    changed: Query<Entity, (With<RefVisual>, With<MeshMaterial3d<NifMaterial>>, Or<(Added<RefVisual>, Changed<VisibilityRange>)>)>,
    all: Query<Entity, (With<RefVisual>, With<MeshMaterial3d<NifMaterial>>)>,
    mut meshes: Query<(&RefVisual, &mut MeshMaterial3d<NifMaterial>, Option<&VisibilityRange>, Option<&DrawClass>)>,
    parents: Query<&ChildOf>,
    marks: Query<(Has<CellEntity>, Has<ActorRig>, Has<DroppedRoot>)>,
    mut known: Local<HashMap<u32, DrawClass>>,
    work: Option<ResMut<crate::quality_probe::Work>>,
) {
    let entities: Vec<Entity> = if distances.is_changed() { all.iter().collect() } else { changed.iter().collect() };
    if let Some(mut work) = work.filter(|_| distances.is_changed()) {
        work.ranges += 1;
        work.range_meshes += entities.len() as u64;
    }
    if distances.is_changed() && !distances.is_added() {
        info!(
            "draw distances: objects {:.0}%, actors {:.0}%, items {:.0}% of {:.0} units ({} meshes)",
            distances.objects * 100.0,
            distances.actors * 100.0,
            distances.items * 100.0,
            distances.reach,
            entities.len()
        );
    }
    for e in entities {
        let Ok((r, mut material, range, class)) = meshes.get_mut(e) else { continue };
        let class = match class {
            Some(c) => *c,
            None => {
                let c = classify(e, r.0, &parents, &marks, world.as_deref(), &mut known);
                commands.entity(e).insert(c);
                c
            }
        };
        let want = distances.distance(class);
        match range {
            // Never had a range (cell fades off): one only when it is to be culled.
            None => {
                if let Some(d) = want {
                    commands.entity(e).insert(fade::drawn_within(Some(d), FADE));
                    // Its pipeline is chosen again, with the dither (fade.rs notes).
                    material.set_changed();
                }
            }
            // Fading in or out: the fade's own range until it rests.
            Some(r) if !fade::at_rest(r) => {}
            Some(r) => {
                let target = fade::drawn_within(want, FADE);
                if *r != target {
                    commands.entity(e).insert(target);
                }
            }
        }
    }
}

/// Set a slider's value and act on it now (the settings page).
pub fn set_part(world: &mut World, class: DrawClass, v: f32) {
    let mut d = world.resource_mut::<DrawDistances>();
    match class {
        DrawClass::Object => d.objects = v,
        DrawClass::Actor => d.actors = v,
        DrawClass::Item => d.items = v,
        DrawClass::Unranged => {}
    }
}

/// The sun's shadows reach `v` of `--shadow-distance` from now.
pub fn set_shadows(world: &mut World, v: f32) {
    let full = {
        let mut d = world.resource_mut::<DrawDistances>();
        d.shadows = v;
        d.shadow_full
    };
    if full <= 0.0 {
        return;
    }
    let on = {
        let mut sky = world.resource_mut::<SkyState>();
        sky.shadow_distance = full * v;
        sky.shadows
    };
    // The cascades again, if the sun casts shadows now.
    if on {
        crate::sky::set_shadows(world, true);
    }
}

/// Grass grows out to `v` of `--grass-distance` (and its fade range) from now
/// (`grass.rs` gives its materials the distances).
pub fn set_grass(world: &mut World, v: f32) {
    let (start, range) = {
        let mut d = world.resource_mut::<DrawDistances>();
        d.grass = v;
        d.grass_full
    };
    let mut g = world.resource_mut::<GrassConfig>();
    g.fade_start = start * v;
    g.fade_range = range * v;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn distances() -> DrawDistances {
        DrawDistances { objects: 1.0, actors: 0.5, items: 0.25, shadows: 1.0, grass: 1.0, reach: full_reach(2), shadow_full: 8000.0, grass_full: (7000.0, 1000.0) }
    }

    #[test]
    fn the_full_distance_is_the_loaded_cells_reach() {
        assert!((full_reach(2) - 3.0 * CELL_SIZE * std::f32::consts::SQRT_2).abs() < 1e-3);
        let d = distances();
        assert_eq!(d.distance(DrawClass::Object), None, "100%: as far as the cells");
        assert_eq!(d.distance(DrawClass::Actor), Some(d.reach * 0.5));
        assert_eq!(d.distance(DrawClass::Item), Some(d.reach * 0.25));
        assert_eq!(d.distance(DrawClass::Unranged), None);
    }

    #[test]
    fn a_resting_range_ends_at_the_distance_and_full_is_the_open_one() {
        let open = fade::drawn(fade::STEPS);
        assert!(fade::drawn_within(None, FADE) == open);
        let r = fade::drawn_within(Some(4000.0), FADE);
        assert!(fade::at_rest(&r) && fade::at_rest(&open));
        assert!(!fade::at_rest(&fade::drawn(8)));
        assert!(r.is_visible_at_all(3999.0) && r.is_visible_at_all(4300.0));
        assert!(!r.is_visible_at_all(4401.0), "culled past the fade");
    }

    #[test]
    fn the_sliders_set_the_grass_and_the_parts() {
        let mut app = App::new();
        app.insert_resource(distances());
        app.insert_resource(GrassConfig { enabled: true, fade_start: 7000.0, fade_range: 1000.0, light: (1.0, 1.0), generate: Default::default() });
        let w = app.world_mut();
        set_grass(w, 0.5);
        let g = w.resource::<GrassConfig>();
        assert_eq!((g.fade_start, g.fade_range), (3500.0, 500.0));
        set_part(w, DrawClass::Actor, 0.3);
        assert_eq!(w.resource::<DrawDistances>().actors, 0.3);
    }
}
