//! An in-engine CPU profile by span (docs/design/frame-rate-audit.md §1), for finding what
//! costs the frames without a sampling profiler (xperf needs elevation;
//! docs/design/streaming-hitches.md §1).
//!
//! `NVF_PLAY_TEST_PROFILE=<seconds>`: every that many real seconds one line
//! `profile: <s> s, <n> frames, frame mean <m> ms, cpu <c> ms a frame; per frame: <span>
//! <ms>, …` (cpu: the process's processor time over every thread, which other programs
//! on a busy machine do not inflate as they do wall times) with the
//! spans that took the most time in the window, summed over every thread and divided by
//! the frames (a system that runs on a worker thread beside others counts its own time,
//! not the frame's). The spans are Bevy's: each system (`system` spans, by the system's
//! name), the schedules, the render graph's nodes and the render app's systems. They exist
//! only in a build with Bevy's `trace` feature (`cargo build --release --features
//! profile`); without it the lines say no spans were seen. The layer is installed only
//! when the variable is set.
//!
//! A frame of 100 ms or more ([`HITCH_MS`], real time from one frame's report to the
//! next; `NVF_PLAY_TEST_PROFILE_HITCH_MS=<ms>` sets another threshold, to see what the
//! occasional 20 ms frames hold) also logs `profile hitch: <ms> ms; spans ms: <span> <ms> (<entries>), …`: the
//! spans that ended in it, over every thread, so a single stall is not lost in a window's
//! means.
//!
//! Each window also logs `profile scene: <n> mesh entities (<v> visible, <c> moved this
//! frame), <m> distinct meshes, <u> distinct materials in use (<a> material assets)`: what
//! the renderer batches by, and how many entities it extracts again because their
//! transform changed. And from the render world `profile batches: <phase> <v> views,
//! <m> multi-drawn sets, <b> batched bins, <u> unbatched bins; …` per binned phase,
//! summed over its views: each multi-drawn set is one indirect draw, each other bin one
//! draw, and the per-view preparing scales with them (graphics-enhanced.md §5.5).
//!
//! `profile outfits:` counts successful NPC spawns by appearance source and committed
//! `follow_inventory` replacements per window, with their total synchronous time in ms.
//! A replacement counts one actor, even if both its armour and weapon change. Failed
//! appearance resolutions and unchanged outfits do not count [design].

use bevy::log::tracing::field::{Field, Visit};
use bevy::log::tracing::span::{Attributes, Id};
use bevy::log::tracing::Subscriber;
use bevy::log::tracing_subscriber::layer::Context;
use bevy::log::tracing_subscriber::registry::LookupSpan;
use bevy::log::tracing_subscriber::Layer;
use bevy::log::BoxedLayer;
use bevy::core_pipeline::core_3d::{AlphaMask3d, Opaque3d};
use bevy::core_pipeline::prepass::{AlphaMask3dPrepass, Opaque3dPrepass};
use bevy::prelude::*;
use bevy::render::render_phase::ViewBinnedRenderPhases;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Spans listed in a line, the most costly first.
const LISTED: usize = 40;

/// A frame at least this long (milliseconds of real time from one report to the next)
/// logs its own spans (`profile hitch:`), which a window's means would hide.
const HITCH_MS: f64 = 100.0;

/// The hitch threshold: [`HITCH_MS`] or `NVF_PLAY_TEST_PROFILE_HITCH_MS`.
fn hitch_ms() -> f64 {
    static MS: OnceLock<f64> = OnceLock::new();
    *MS.get_or_init(|| {
        std::env::var("NVF_PLAY_TEST_PROFILE_HITCH_MS").ok().and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| *v > 0.0).unwrap_or(HITCH_MS)
    })
}

/// Spans listed in a hitch's line.
const LISTED_HITCH: usize = 25;

/// The window in seconds, when `NVF_PLAY_TEST_PROFILE` asks for a profile.
pub fn interval() -> Option<f32> {
    static EVERY: OnceLock<Option<f32>> = OnceLock::new();
    *EVERY.get_or_init(|| std::env::var("NVF_PLAY_TEST_PROFILE").ok().and_then(|v| v.trim().parse::<f32>().ok()).filter(|s| *s > 0.0))
}

/// Time per span key in the current window: (seconds, entries).
static TOTALS: Mutex<Option<HashMap<String, (f64, u64)>>> = Mutex::new(None);

/// NPC outfit work in this report window. Callers accumulate locally, then add once
/// per system call; no per-actor locks and no timings when profiling is disabled.
#[derive(Default)]
struct Outfits {
    equipment: usize,
    record: usize,
    replacements: usize,
    replacement_ms: f64,
}

static OUTFITS: Mutex<Outfits> = Mutex::new(Outfits { equipment: 0, record: 0, replacements: 0, replacement_ms: 0.0 });

pub fn actor_spawns(equipment: usize, record: usize) {
    if interval().is_none() || equipment + record == 0 {
        return;
    }
    let mut o = OUTFITS.lock().unwrap_or_else(|e| e.into_inner());
    o.equipment += equipment;
    o.record += record;
}

pub fn outfit_replacements(count: usize, ms: f64) {
    if interval().is_none() || count == 0 {
        return;
    }
    let mut o = OUTFITS.lock().unwrap_or_else(|e| e.into_inner());
    o.replacements += count;
    o.replacement_ms += ms;
}

/// The render world's pipeline cache, as of its last frame: pipelines made, and those
/// still waiting to compile ([`pipeline_counts`]); a hitch line gives both, so a stall can
/// be told apart from one that waited on pipelines compiling (2026-10-28: material writes
/// and the window's frame took 100-500 ms on long tours with nothing else in them).
static PIPELINES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static PIPELINES_WAITING: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Time per span key since the last frame's report: (seconds, entries).
static FRAME: Mutex<Option<HashMap<String, (f64, u64)>>> = Mutex::new(None);

/// The layer, when the profile is asked for.
pub fn layer() -> Option<BoxedLayer> {
    interval()?;
    Some(Box::new(SpanTimes))
}

struct SpanTimes;

/// A span's key (its name, with the `name` field of Bevy's system spans) and when it was
/// last entered.
struct Timing {
    key: String,
    entered: Option<Instant>,
}

/// Reads the `name` field of a span.
struct NameField(Option<String>);

impl Visit for NameField {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "name" {
            self.0 = Some(value.to_string());
        }
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "name" && self.0.is_none() {
            self.0 = Some(format!("{value:?}").trim_matches('"').to_string());
        }
    }
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for SpanTimes {
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut name = NameField(None);
        attrs.record(&mut name);
        let base = attrs.metadata().name();
        let key = match name.0 {
            Some(n) => short_name(&n),
            None => base.to_string(),
        };
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(Timing { key, entered: None });
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            if let Some(t) = span.extensions_mut().get_mut::<Timing>() {
                t.entered = Some(Instant::now());
            }
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let mut ext = span.extensions_mut();
        let Some(t) = ext.get_mut::<Timing>() else { return };
        let Some(at) = t.entered.take() else { return };
        let secs = at.elapsed().as_secs_f64();
        let mut totals = TOTALS.lock().unwrap_or_else(|e| e.into_inner());
        let e = totals.get_or_insert_with(HashMap::new).entry(t.key.clone()).or_default();
        e.0 += secs;
        e.1 += 1;
        drop(totals);
        let mut frame = FRAME.lock().unwrap_or_else(|e| e.into_inner());
        let e = frame.get_or_insert_with(HashMap::new).entry(t.key.clone()).or_default();
        e.0 += secs;
        e.1 += 1;
    }
}

/// A system's type path without its modules: `nvforever::residents::drive` →
/// `residents::drive` (the last two segments outside generics).
pub fn short_name(full: &str) -> String {
    let head = full.split('<').next().unwrap_or(full);
    let parts: Vec<&str> = head.split("::").collect();
    let keep = parts.len().saturating_sub(2);
    let mut s = parts[keep..].join("::");
    if head.len() < full.len() {
        s.push_str("<…>");
    }
    s
}

pub struct ProfilePlugin;

impl Plugin for ProfilePlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("NVF_PLAY_TEST_ACTOR_NO_SHADOWS").is_ok_and(|v| v == "1") {
            info!("test aid NVF_PLAY_TEST_ACTOR_NO_SHADOWS: actors cast no shadows");
            app.add_systems(Last, actors_cast_no_shadows);
        }
        let Some(every) = interval() else { return };
        info!("test aid NVF_PLAY_TEST_PROFILE: a span profile every {every} s");
        // GPU time per render pass (timestamp queries, Vulkan and DX12): `profile gpu:`.
        crate::graphics::ensure_render_diagnostics(app);
        app.insert_resource(ProfileWindow { every, ..default() }).init_resource::<AssetChanges>().add_systems(Last, (count_asset_changes, report).chain());
        if let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) {
            render.add_systems(bevy::render::Render, pipeline_counts.in_set(bevy::render::RenderSystems::Render));
            render.add_systems(bevy::render::Render, (move |mut last: Local<Option<Instant>>, shadow: Res<ViewBinnedRenderPhases<bevy::pbr::Shadow>>, opaque: Res<ViewBinnedRenderPhases<Opaque3d>>, mask: Res<ViewBinnedRenderPhases<AlphaMask3d>>, prepass: Res<ViewBinnedRenderPhases<Opaque3dPrepass>>, mask_prepass: Res<ViewBinnedRenderPhases<AlphaMask3dPrepass>>| {
                // The render world has no clock of its own: real time.
                let now = Instant::now();
                let at = *last.get_or_insert(now);
                if now.duration_since(at).as_secs_f32() < every {
                    return;
                }
                *last = Some(now);
                info!(
                    "profile batches: {}; {}; {}; {}; {}",
                    batches("shadow", &shadow),
                    batches("opaque", &opaque),
                    batches("alpha-mask", &mask),
                    batches("opaque prepass", &prepass),
                    batches("alpha-mask prepass", &mask_prepass)
                );
            })
            .in_set(bevy::render::RenderSystems::Render));
        }
    }
}

/// Keep the pipeline counts for the main world's hitch lines.
fn pipeline_counts(cache: Res<bevy::render::render_resource::PipelineCache>) {
    use std::sync::atomic::Ordering::Relaxed;
    PIPELINES.store(cache.pipelines().count() as u64, Relaxed);
    PIPELINES_WAITING.store(cache.waiting_pipelines().count() as u64, Relaxed);
}

/// `<phase> <v> views, <m> multi-drawn sets, <b> batched bins, <u> unbatched bins`, summed
/// over the phase's views.
fn batches<BPI: bevy::render::render_phase::BinnedPhaseItem>(name: &str, phases: &ViewBinnedRenderPhases<BPI>) -> String {
    let (mut m, mut b, mut u) = (0, 0, 0);
    for phase in phases.values() {
        m += phase.multidrawable_meshes.len();
        b += phase.batchable_meshes.len();
        u += phase.unbatchable_meshes.len();
    }
    format!("{name} {} views, {m} multi-drawn sets, {b} batched bins, {u} unbatched bins", phases.len())
}

/// The asset changes of a window, for `profile changes:`: each made, changed or dropped
/// mesh, image and game material is sent to the GPU again (its staging memory included);
/// the night stalls came with GPU memory made and freed hundreds of times a second
/// (Nsight Systems, 2026-10-29), so this says what the frames send.
#[derive(Resource, Default)]
struct AssetChanges {
    /// (added, modified, removed) per kind: meshes, images, materials.
    counts: [[u64; 3]; 3],
    /// Bytes of the added and modified meshes' vertices and the images' pixels.
    mesh_bytes: u64,
    image_bytes: u64,
}

/// Which of (made, changed, dropped) an asset event counts as.
fn slot<A: Asset>(e: &AssetEvent<A>) -> Option<usize> {
    match e {
        AssetEvent::Added { .. } => Some(0),
        AssetEvent::Modified { .. } => Some(1),
        AssetEvent::Removed { .. } | AssetEvent::Unused { .. } => Some(2),
        _ => None,
    }
}

fn count_asset_changes(
    mut c: ResMut<AssetChanges>,
    mut mesh_events: MessageReader<AssetEvent<Mesh>>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    mut material_events: MessageReader<AssetEvent<nvf_assets::NifMaterial>>,
    meshes: Res<Assets<Mesh>>,
    images: Res<Assets<Image>>,
) {
    for e in mesh_events.read() {
        if let Some(i) = slot(e) {
            c.counts[0][i] += 1;
        }
        if let AssetEvent::Added { id } | AssetEvent::Modified { id } = e {
            if let Some(m) = meshes.get(*id) {
                c.mesh_bytes += (m.count_vertices() * m.get_vertex_size() as usize) as u64;
            }
        }
    }
    for e in image_events.read() {
        if let Some(i) = slot(e) {
            c.counts[1][i] += 1;
        }
        if let AssetEvent::Added { id } | AssetEvent::Modified { id } = e {
            if let Some(d) = images.get(*id).and_then(|im| im.data.as_ref()) {
                c.image_bytes += d.len() as u64;
            }
        }
    }
    for e in material_events.read() {
        if let Some(i) = slot(e) {
            c.counts[2][i] += 1;
        }
    }
}

#[derive(Resource, Default)]
struct ProfileWindow {
    every: f32,
    /// The process's processor time at the window's start (seconds).
    cpu_at: Option<f64>,
    since: f32,
    frames: u32,
    frame_secs: f32,
}

/// Test aid `NVF_PLAY_TEST_ACTOR_NO_SHADOWS=1`: every mesh under an actor's skeleton stops
/// casting shadows (still drawn), for measuring what the actors' shadows cost.
fn actors_cast_no_shadows(
    mut commands: Commands,
    meshes: Query<Entity, (With<Mesh3d>, Without<bevy::light::NotShadowCaster>)>,
    parents: Query<&ChildOf>,
    rigs: Query<(), With<crate::limbs::ActorRig>>,
    mut checked: Local<std::collections::HashSet<Entity>>,
    mut marked: Local<usize>,
) {
    let before = *marked;
    for e in &meshes {
        if !checked.insert(e) {
            continue;
        }
        if parents.iter_ancestors(e).any(|a| rigs.contains(a)) {
            commands.entity(e).insert(bevy::light::NotShadowCaster);
            *marked += 1;
        }
    }
    if *marked / 100 != before / 100 || (before == 0 && *marked > 0) {
        info!("test aid NVF_PLAY_TEST_ACTOR_NO_SHADOWS: {} actor meshes cast no shadows, {} rigs", *marked, rigs.iter().count());
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn report(
    time: Res<Time<Real>>,
    mut w: ResMut<ProfileWindow>,
    meshes: Query<(&Mesh3d, Option<&MeshMaterial3d<nvf_assets::NifMaterial>>, &ViewVisibility)>,
    moved: Query<(), (With<Mesh3d>, Changed<GlobalTransform>)>,
    materials: Res<Assets<nvf_assets::NifMaterial>>,
    mut last: Local<Option<Instant>>,
    loading: Option<Res<nvf_play::loading::Loading>>,
    mut pipelines_before: Local<u64>,
    (diagnostics, mut changes): (Res<bevy::diagnostic::DiagnosticsStore>, ResMut<AssetChanges>),
    (suns, visibles): (
        Query<(Entity, &bevy::camera::visibility::CascadesVisibleEntities), With<DirectionalLight>>,
        Query<(&InheritedVisibility, Has<Mesh3d>, Has<Children>)>,
    ),
    (transforms, changed_transforms, same_transforms): (
        Query<(), With<Transform>>,
        Query<(Entity, Has<Mesh3d>, Option<&Name>), Changed<Transform>>,
        Query<Ref<GlobalTransform>, Changed<Transform>>,
    ),
) {
    let pipelines = PIPELINES.load(std::sync::atomic::Ordering::Relaxed);
    let new_pipelines = pipelines.saturating_sub(*pipelines_before);
    *pipelines_before = pipelines;
    // A slow frame's own spans: those ended since the last report, whatever the thread.
    let now = Instant::now();
    let frame = FRAME.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    if let Some(at) = last.replace(now) {
        let ms = now.duration_since(at).as_secs_f64() * 1000.0;
        if ms >= hitch_ms() {
            let line = format!(
                "{}; pipelines {pipelines} ({new_pipelines} new since the frame before, {} waiting)",
                hitch_line(ms, &frame),
                PIPELINES_WAITING.load(std::sync::atomic::Ordering::Relaxed)
            );
            if loading.as_deref().is_some_and(nvf_play::loading::Loading::unseen_frames) {
                // Not seen: kept apart from the hitches (`profile hitch (behind a loading screen):`).
                info!("{}", line.replacen("profile hitch:", "profile hitch (behind a loading screen):", 1));
            } else {
                info!("{line}");
            }
        }
    }
    let dt = time.delta_secs();
    w.since += dt;
    w.frames += 1;
    w.frame_secs += dt;
    if w.since < w.every {
        return;
    }
    let totals = TOTALS.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    let frames = w.frames.max(1) as f64;
    let cpu = process_cpu_secs();
    let cpu_ms = cpu.zip(w.cpu_at).map(|(now, at)| (now - at) * 1000.0 / frames);
    let text = line(w.since, w.frames, w.frame_secs, &totals, frames);
    info!("{}", match cpu_ms {
        Some(c) => text.replacen(" ms; ", &format!(" ms, cpu {c:.1} ms a frame; "), 1),
        None => text,
    });
    let (mut visible, mut ids, mut mats) = (0usize, std::collections::HashSet::new(), std::collections::HashSet::new());
    for (mesh, material, vis) in &meshes {
        visible += usize::from(vis.get());
        ids.insert(mesh.0.id());
        if let Some(m) = material {
            mats.insert(m.0.id());
        }
    }
    info!(
        "profile scene: {} mesh entities ({visible} visible, {} moved this frame), {} distinct meshes, {} distinct materials in use ({} material assets)",
        meshes.iter().count(),
        moved.iter().count(),
        ids.len(),
        mats.len(),
        materials.len()
    );
    // Shadow casters each cascade of each directional light drew this frame: whether a nearer,
    // smaller cascade really holds fewer casters than the far one.
    let shadows: Vec<String> = suns
        .iter()
        .flat_map(|(light, c)| c.entities.values().map(move |cascades| {
            let counts: Vec<String> = cascades.iter().map(|v| v.entities.len().to_string()).collect();
            format!("light {light}: {}", counts.join("/"))
        }))
        .collect();
    // Every entity with visibility is walked by Bevy's visibility systems each frame, meshes or not.
    let (mut with_vis, mut shown, mut bare) = (0usize, 0usize, 0usize);
    for (inherited, mesh, children) in &visibles {
        with_vis += 1;
        shown += usize::from(inherited.get());
        bare += usize::from(!mesh && !children);
    }
    info!("profile visibility: {with_vis} entities with visibility ({shown} not hidden), {bare} with neither a mesh nor children");
    // Transforms written this frame (each makes its tree dirty for propagation), how many
    // of them carry a mesh, the most common names, and how many ended where they were.
    let mut names: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let (mut changed, mut with_mesh) = (0usize, 0usize);
    for (_, mesh, name) in &changed_transforms {
        changed += 1;
        with_mesh += usize::from(mesh);
        let key = name.map(|n| n.as_str().split(|c: char| c.is_ascii_digit()).next().unwrap_or("").to_string()).unwrap_or_else(|| "(no name)".into());
        *names.entry(key).or_default() += 1;
    }
    let unmoved = same_transforms.iter().filter(|g| !g.is_changed()).count();
    let mut top: Vec<(String, usize)> = names.into_iter().collect();
    top.sort_by_key(|t| std::cmp::Reverse(t.1));
    let top: Vec<String> = top.iter().take(8).map(|(n, c)| format!("{n} {c}")).collect();
    info!(
        "profile transforms: {changed} of {} written this frame ({with_mesh} with a mesh; {unmoved} not yet propagated), most: {}",
        transforms.iter().count(),
        top.join(", ")
    );
    info!("profile shadows: casters per cascade, nearest first: {}", shadows.join("; "));
    let per = |n: u64| n as f64 / frames;
    let o = std::mem::take(&mut *OUTFITS.lock().unwrap_or_else(|e| e.into_inner()));
    info!(
        "profile outfits: {} NPCs spawned ({} simulation equipment, {} record fallback); {} follow_inventory replacements ({:.2} ms)",
        o.equipment + o.record, o.equipment, o.record, o.replacements, o.replacement_ms
    );
    let c = std::mem::take(&mut *changes);
    info!(
        "profile changes: per frame: meshes {:.1} made {:.1} changed {:.1} dropped ({:.2} MB of vertices), images {:.1} made {:.1} changed {:.1} dropped ({:.2} MB of pixels), materials {:.1} made {:.1} changed {:.1} dropped",
        per(c.counts[0][0]),
        per(c.counts[0][1]),
        per(c.counts[0][2]),
        per(c.mesh_bytes) / 1e6,
        per(c.counts[1][0]),
        per(c.counts[1][1]),
        per(c.counts[1][2]),
        per(c.image_bytes) / 1e6,
        per(c.counts[2][0]),
        per(c.counts[2][1]),
        per(c.counts[2][2]),
    );
    // The render passes' GPU time (Bevy's render diagnostics, averaged over their last
    // samples), the costliest first: which pass grows when the frame waits on the GPU.
    let mut gpu: Vec<(String, f64)> = diagnostics
        .iter()
        .filter(|d| d.path().as_str().ends_with("/elapsed_gpu"))
        .filter_map(|d| Some((d.path().as_str().trim_start_matches("render/").trim_end_matches("/elapsed_gpu").to_string(), d.average()?)))
        .collect();
    gpu.sort_by(|a, b| b.1.total_cmp(&a.1));
    if !gpu.is_empty() {
        let listed: Vec<String> = gpu.iter().take(16).map(|(k, ms)| format!("{k} {ms:.2}")).collect();
        info!("profile gpu: ms a frame: {}", listed.join(", "));
    }
    *w = ProfileWindow { every: w.every, cpu_at: cpu, ..default() };
}

/// The line for a window.
fn line(secs: f32, frames: u32, frame_secs: f32, totals: &HashMap<String, (f64, u64)>, per: f64) -> String {
    let mut spans: Vec<(&String, &(f64, u64))> = totals.iter().collect();
    spans.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0));
    let listed: Vec<String> = spans.iter().take(LISTED).map(|(k, (s, _))| format!("{k} {:.2}", s * 1000.0 / per)).collect();
    let mean = if frames > 0 { frame_secs * 1000.0 / frames as f32 } else { 0.0 };
    if listed.is_empty() {
        format!("profile: {secs:.1} s, {frames} frames, frame mean {mean:.1} ms; no spans seen (build with --features profile)")
    } else {
        format!("profile: {secs:.1} s, {frames} frames, frame mean {mean:.1} ms; per frame ms: {}", listed.join(", "))
    }
}

/// The line for one slow frame: `profile hitch: <ms> ms; spans ms: <span> <ms> (<n>), …`,
/// each span's time in that frame and how often it was entered.
fn hitch_line(ms: f64, frame: &HashMap<String, (f64, u64)>) -> String {
    let mut spans: Vec<(&String, &(f64, u64))> = frame.iter().collect();
    spans.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0));
    let listed: Vec<String> = spans.iter().take(LISTED_HITCH).map(|(k, (s, n))| format!("{k} {:.1} ({n})", s * 1000.0)).collect();
    if listed.is_empty() {
        format!("profile hitch: {ms:.0} ms; no spans seen (build with --features profile)")
    } else {
        format!("profile hitch: {ms:.0} ms; spans ms: {}", listed.join(", "))
    }
}

/// The processor time the process has used so far, user and kernel, seconds.
#[cfg(windows)]
fn process_cpu_secs() -> Option<f64> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let mut t = [FILETIME::default(); 4];
    // SAFETY: the pseudo handle of the current process needs no closing; the four out
    // pointers are to live FILETIMEs.
    let ok = unsafe { GetProcessTimes(GetCurrentProcess(), &mut t[0], &mut t[1], &mut t[2], &mut t[3]) };
    let secs = |f: &FILETIME| ((u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)) as f64 * 1e-7;
    (ok != 0).then(|| secs(&t[2]) + secs(&t[3]))
}

#[cfg(not(windows))]
fn process_cpu_secs() -> Option<f64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_names_keep_their_module() {
        assert_eq!(short_name("nvforever::residents::drive"), "residents::drive");
        assert_eq!(short_name("drive"), "drive");
        assert_eq!(short_name("bevy_ecs::event::update<nvf_play::Foo>"), "event::update<…>");
    }

    #[test]
    fn a_line_lists_the_costliest_first_per_frame() {
        let mut t = HashMap::new();
        t.insert("a".to_string(), (0.010, 10));
        t.insert("b".to_string(), (0.100, 10));
        let l = line(1.0, 10, 1.0, &t, 10.0);
        assert!(l.contains("per frame ms: b 10.00, a 1.00"), "{l}");
        assert!(line(1.0, 0, 0.0, &HashMap::new(), 1.0).contains("no spans seen"));
    }

    #[test]
    fn a_hitch_lists_its_own_spans_costliest_first() {
        let mut t = HashMap::new();
        t.insert("a".to_string(), (0.010, 1));
        t.insert("b".to_string(), (0.500, 3));
        let l = hitch_line(612.4, &t);
        assert!(l.starts_with("profile hitch: 612 ms; spans ms: b 500.0 (3), a 10.0 (1)"), "{l}");
        assert!(hitch_line(150.0, &HashMap::new()).contains("no spans seen"));
    }
}
