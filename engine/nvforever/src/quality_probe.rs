//! Main-menu quality edit measurements [design, #2248; docs/spec-notes/quality-presets.md].
//! Counts describe work during the window, including unrelated UI/streaming activity;
//! they do not assert that an edit caused an asset reload or a driver shader compile.

use crate::{
    pipeline_gate::PipelineGate,
    quality,
    world::{Streamer, WorldRoot},
};
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowMode};
use nvf_play::settings::{EditPhase, EditProbe, Settings, StoreAttempts};
use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

/// Follow each edit for three real seconds, retaining a frame that crosses the deadline
/// [design, #2248]. This diagnostic changes no gameplay or renderer defaults.
const FOLLOW_SECS: f64 = 3.0;

/// Actual application passes, accumulated independently of the settings setters
/// [design, #2248: graphics::apply, draw_distance::apply_ranges].
#[derive(Resource, Default, Clone)]
pub(crate) struct Work {
    pub graphics: u64,
    pub ranges: u64,
    pub range_meshes: u64,
    pub shadow_updates: u64,
    assets: [[u64; 3]; 2],
    roots: u64,
    visual_parts: u64,
    cameras: u64,
    window_changes: u64,
}

#[derive(Default, Clone)]
struct Counts {
    work: Work,
    cells: u64,
    references: u64,
    writes: u64,
    pipelines: u32,
    render_frames: u32,
}

struct Edit {
    key: String,
    started: Instant,
    before: Counts,
    depth: usize,
}

struct WindowSample {
    key: String,
    value: String,
    started: Instant,
    setter_ms: f64,
    finished: Instant,
    before: Counts,
    longest_ms: f64,
    frames: u32,
    pending_peak: u32,
    menu_remained_open: bool,
}

impl WindowSample {
    fn frame(&mut self, now: Instant, interval_ms: f64, pending: u32, menu_open: bool) -> bool {
        self.longest_ms = self.longest_ms.max(interval_ms);
        self.pending_peak = self.pending_peak.max(pending);
        self.frames += 1;
        self.menu_remained_open &= menu_open;
        now.duration_since(self.finished).as_secs_f64() >= FOLLOW_SECS
    }
}

#[derive(Resource, Default)]
struct Probe {
    edit: Option<Edit>,
    windows: Vec<WindowSample>,
    previous_frame: Option<Instant>,
}

fn menu_open(world: &World) -> bool {
    world.get_resource::<nvf_play::main_menu::MainMenu>().is_some_and(|m| m.open)
}

fn counts(world: &World) -> Counts {
    let mut counts = Counts { work: world.resource::<Work>().clone(), ..default() };
    if let Some(streamer) = world.get_resource::<Streamer>() {
        counts.cells = streamer.stats.cell_starts;
        counts.references = streamer.stats.reference_attempts;
    }
    if let Some(writes) = world.get_resource::<StoreAttempts>() {
        counts.writes = writes.0.load(Relaxed);
    }
    if let Some(gate) = world.get_resource::<PipelineGate>() {
        counts.pipelines = gate.asked();
        counts.render_frames = gate.frames();
    }
    counts
}

fn edit_probe(world: &mut World, key: &str, phase: EditPhase) {
    match phase {
        EditPhase::Begin => {
            if let Some(edit) = &mut world.resource_mut::<Probe>().edit {
                edit.depth += 1;
                return;
            }
            if !menu_open(world) || !(key == quality::QUALITY_KEY || quality::MAPPED.iter().any(|m| m.row == key)) {
                return;
            }
            let edit = Edit { key: key.to_string(), started: Instant::now(), before: counts(world), depth: 1 };
            world.resource_mut::<Probe>().edit = Some(edit);
            info!("quality measure begin: key={key} main_menu=true follow_secs={FOLLOW_SECS:.0}");
        }
        EditPhase::End { applied } => {
            let edit = {
                let mut probe = world.resource_mut::<Probe>();
                let Some(edit) = &mut probe.edit else { return };
                edit.depth -= 1;
                if edit.depth != 0 {
                    return;
                }
                probe.edit.take().unwrap()
            };
            if !applied {
                info!("quality measure cancelled: key={}", edit.key);
                return;
            }
            let settings = world.resource::<Settings>();
            let value = settings.index(&edit.key).map_or_else(String::new, |i| {
                let s = settings.list()[i];
                if let Some(c) = settings.choices(i) {
                    (c.chosen)(world)
                } else if let Some(slider) = s.slider {
                    slider.text((slider.get)(world))
                } else if (s.get)(world) {
                    "on".to_string()
                } else {
                    "off".to_string()
                }
            });
            let finished = Instant::now();
            let setter_ms = finished.duration_since(edit.started).as_secs_f64() * 1000.0;
            let pending_peak = world.get_resource::<PipelineGate>().map_or(0, PipelineGate::busy);
            world.resource_mut::<Probe>().windows.push(WindowSample {
                key: edit.key,
                value,
                started: edit.started,
                setter_ms,
                finished,
                before: edit.before,
                longest_ms: 0.0,
                frames: 0,
                pending_peak,
                menu_remained_open: true,
            });
        }
    }
}

#[derive(Clone, PartialEq)]
struct WindowState(Entity, UVec2, WindowMode, PresentMode, u32);

fn asset_slot<A: Asset>(event: &AssetEvent<A>) -> Option<usize> {
    match event {
        AssetEvent::Added { .. } => Some(0),
        AssetEvent::Modified { .. } => Some(1),
        AssetEvent::Removed { .. } => Some(2),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn collect(
    mut work: ResMut<Work>,
    mut meshes: MessageReader<AssetEvent<Mesh>>,
    mut images: MessageReader<AssetEvent<Image>>,
    roots: Query<(), Added<WorldRoot>>,
    visuals: Query<(), Added<nvf_play::RefVisual>>,
    cameras: Query<(), Added<Camera>>,
    windows: Query<(Entity, &Window)>,
    mut previous: Local<Option<Vec<WindowState>>>,
) {
    for event in meshes.read() {
        if let Some(slot) = asset_slot(event) {
            work.assets[0][slot] += 1;
        }
    }
    for event in images.read() {
        if let Some(slot) = asset_slot(event) {
            work.assets[1][slot] += 1;
        }
    }
    work.roots += roots.iter().count() as u64;
    work.visual_parts += visuals.iter().count() as u64;
    work.cameras += cameras.iter().count() as u64;
    let mut state: Vec<_> = windows
        .iter()
        .map(|(e, w)| WindowState(e, UVec2::new(w.resolution.physical_width(), w.resolution.physical_height()), w.mode, w.present_mode, w.resolution.scale_factor().to_bits()))
        .collect();
    state.sort_by_key(|s| s.0);
    if previous.as_ref().is_some_and(|p| *p != state) {
        work.window_changes += 1;
    }
    *previous = Some(state);
}

/// First-to-First wall time includes the edit's whole frame and render backpressure;
/// virtual time's pause/clamp cannot hide a hitch [design, #2248].
fn frame(world: &mut World) {
    let now = Instant::now();
    let interval_ms = {
        let mut probe = world.resource_mut::<Probe>();
        let previous = probe.previous_frame.replace(now).unwrap_or(now);
        if probe.windows.is_empty() {
            return;
        }
        now.duration_since(previous).as_secs_f64() * 1000.0
    };
    let after = counts(world);
    let pending = world.get_resource::<PipelineGate>().map_or(0, PipelineGate::busy);
    let open = menu_open(world);
    let mut probe = world.resource_mut::<Probe>();
    probe.windows.retain_mut(|window| {
        if !window.frame(now, interval_ms, pending, open) {
            return true;
        }
        let b = &window.before;
        let a = &after;
        let delta = |a: u64, b: u64| a.saturating_sub(b);
        let assets = |kind: usize| -> Vec<u64> { (0..3).map(|i| delta(a.work.assets[kind][i], b.work.assets[kind][i])).collect() };
        info!(
            "quality measure: key={} value={} main_menu={} elapsed_secs={:.3} frames={} longest_frame_ms={:.3} setter_ms={:.3} cell_starts={} reference_attempts={} world_roots={} visual_parts={} camera_spawns={} shadow_updates={} graphics_applies={} range_passes={} range_meshes={} pipelines_asked={} pipelines_pending_peak={} render_frames={} mesh_events={:?} image_events={:?} window_changes={} ini_write_attempts={}",
            window.key, window.value, window.menu_remained_open, now.duration_since(window.started).as_secs_f64(), window.frames,
            window.longest_ms, window.setter_ms, delta(a.cells, b.cells), delta(a.references, b.references),
            delta(a.work.roots, b.work.roots), delta(a.work.visual_parts, b.work.visual_parts),
            delta(a.work.cameras, b.work.cameras), delta(a.work.shadow_updates, b.work.shadow_updates),
            delta(a.work.graphics, b.work.graphics), delta(a.work.ranges, b.work.ranges), delta(a.work.range_meshes, b.work.range_meshes),
            a.pipelines.wrapping_sub(b.pipelines), window.pending_peak, a.render_frames.wrapping_sub(b.render_frames),
            assets(0), assets(1), delta(a.work.window_changes, b.work.window_changes), delta(a.writes, b.writes),
        );
        false
    });
}

fn watching(menu: Option<Res<nvf_play::main_menu::MainMenu>>, probe: Res<Probe>) -> bool {
    menu.is_some_and(|m| m.open) || !probe.windows.is_empty()
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<Work>()
        .init_resource::<Probe>()
        .init_resource::<StoreAttempts>()
        .insert_resource(EditProbe(edit_probe))
        .add_systems(First, frame)
        .add_systems(Last, collect.run_if(watching));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn sample(started: Instant) -> WindowSample {
        WindowSample {
            key: "quality".into(),
            value: "test".into(),
            started,
            setter_ms: 0.0,
            finished: started,
            before: Counts::default(),
            longest_ms: 0.0,
            frames: 0,
            pending_peak: 0,
            menu_remained_open: true,
        }
    }

    #[test]
    fn stalled_deadline_frame_is_retained_and_overlapping_windows_are_independent() {
        let start = Instant::now();
        let mut first = sample(start);
        let mut second = sample(start + Duration::from_secs(2));
        let now = start + Duration::from_millis(3500);
        assert!(first.frame(now, 1700.0, 4, true));
        assert!(!second.frame(now, 1700.0, 4, true));
        assert_eq!(first.longest_ms, 1700.0);
        assert_eq!(second.longest_ms, 1700.0);
        assert!(second.frame(start + Duration::from_secs(6), 2500.0, 0, false));
        assert_eq!(second.pending_peak, 4);
        assert_eq!(second.longest_ms, 2500.0);
        assert!(!second.menu_remained_open);
    }

    #[test]
    fn a_slow_setter_keeps_three_seconds_of_follow_up_and_its_own_frame() {
        let start = Instant::now();
        let mut window = sample(start);
        window.finished = start + Duration::from_secs(5);
        assert!(!window.frame(start + Duration::from_secs(6), 6000.0, 1, true));
        assert!(window.frame(start + Duration::from_secs(8), 2000.0, 0, true));
        assert_eq!(window.longest_ms, 6000.0);
    }

    #[test]
    fn nested_preset_rows_make_one_sample_and_failed_edits_release_the_probe() {
        let mut app = App::new();
        register(&mut app);
        app.init_resource::<Settings>().init_resource::<nvf_play::main_menu::MainMenu>();
        app.world_mut().resource_mut::<nvf_play::main_menu::MainMenu>().open = true;
        let world = app.world_mut();
        edit_probe(world, "quality", EditPhase::Begin);
        edit_probe(world, "glow", EditPhase::Begin);
        edit_probe(world, "glow", EditPhase::End { applied: true });
        assert!(world.resource::<Probe>().windows.is_empty());
        edit_probe(world, "quality", EditPhase::End { applied: true });
        assert_eq!(world.resource::<Probe>().windows.len(), 1);
        edit_probe(world, "quality", EditPhase::Begin);
        edit_probe(world, "quality", EditPhase::End { applied: false });
        assert!(world.resource::<Probe>().edit.is_none());
        assert_eq!(world.resource::<Probe>().windows.len(), 1);
        edit_probe(world, "fov", EditPhase::Begin);
        edit_probe(world, "fov", EditPhase::End { applied: true });
        assert!(world.resource::<Probe>().edit.is_none());
        assert_eq!(world.resource::<Probe>().windows.len(), 1);
        world.resource_mut::<nvf_play::main_menu::MainMenu>().open = false;
        edit_probe(world, "glow", EditPhase::Begin);
        edit_probe(world, "glow", EditPhase::End { applied: true });
        assert_eq!(world.resource::<Probe>().windows.len(), 1);
    }
}
