//! The stress tests' samples (docs/design/stress-tests.md; the runner is `tools/stress`).
//!
//! `NVF_PLAY_TEST_SAMPLE=<seconds>`: every that many real seconds one line
//! `stress sample: t <s> s, frames <n>, frame mean <m> ms, frame worst <w> ms, entities <n>,
//! refs drawn <n>, meshes <n>, images <n>, materials <n>, models cached <n>, textures
//! cached <n>, audio <n>, cells <n>,
//! bodies <fixed>/<kinematic>/<dynamic>, bodies awake <n>, contact pairs <n>, manifolds <n>,
//! solver contacts <n>, joints <n>, solver ms <a>/<r>/<d>/<i>/<c>/<s>, actors <n>,
//! nan actors <n>, ragdolls <n>,
//! projectiles <n>[, gpu allocated <MB>, gpu reserved <MB>, gpu blocks <n>], at <place>`
//! with what the engine holds now and where the camera is (the crash report's place text);
//! `bodies awake` to `joints` are the physics solver's load now, and `solver ms` rapier's
//! stage timers summed since the start (`fnv_phys::Stats::solver_us`: set-up, solve,
//! collision detection, islands, CCD, the whole step; 0 outside the profile build) (#1798),
//! and `stress gpu labels: <label> <MB> in <n>; …` (the GPU allocator's report, where the
//! backend gives one: [`gpu_memory`]). At the exit, `stress frames: <n> frames, mean <m> ms,
//! 1% low <l> ms (<fps> fps), worst <w> ms` over the run after its start-up ([`frame_stats`]),
//! and `stress actors: <n> drawn in <f> frames (<ms> ms, most in a frame <m> ms); those
//! frames mean <m> ms, worst <w> ms, <k> over 100 ms`.
//!
//! Start-up (the frames until the start's cells are all in, `Streamer::settled`; the first
//! takes the app's build and the Startup systems, 2.5 s in the stress runs of 2026-10-18)
//! is left out of the samples' and the run's frame times and logged once on its own:
//! `stress start-up: <n> frames, <ms> ms, slowest <w> ms`. After it, a frame slower than
//! [`HITCH_MS`] logs `stress hitch: <ms> ms at t <s> s, <k> cells loaded in it (<ms> ms),
//! <a> actors drawn in it (<ms> ms), at <place>`, so a stall can be told from a cell load
//! or from actors coming into view (`readahead::clocked`; actors a cell draws count in
//! both). A frame behind a loading screen (`Loading::unseen_frames`) is not seen: it is
//! left out of the frame times and its stall logged apart, `stress hitch (behind a
//! loading screen): …`, with the count of such frames at the exit's `stress frames:`
//! line [observed 2026-10-27: every `world::stream` stall after a door or teleport on a
//! soak run was behind the screen, and counted as hitches]. The process's memory is the
//! runner's to read (it has the process).

use crate::limbs::ActorRig;
use bevy::audio::AudioSink;
use bevy::prelude::*;
use nvf_play::phys::PhysState;
use std::collections::HashMap;
use nvf_play::RefVisual;

pub struct StressSamplePlugin;

impl StressSamplePlugin {
    /// The sampling interval, when `NVF_PLAY_TEST_SAMPLE` asks for samples.
    pub fn interval() -> Option<f32> {
        std::env::var("NVF_PLAY_TEST_SAMPLE").ok().and_then(|v| v.trim().parse::<f32>().ok()).filter(|s| *s > 0.0)
    }
}

impl Plugin for StressSamplePlugin {
    fn build(&self, app: &mut App) {
        let Some(every) = Self::interval() else { return };
        info!("test aid NVF_PLAY_TEST_SAMPLE: a stress sample every {every} s");
        app.insert_resource(Samples { every, ..default() }).add_systems(Last, ((sample, report).chain(), census, cache_census));
        if let Some(render) = app.get_sub_app_mut(bevy::render::RenderApp) {
            render.add_systems(bevy::render::Render, render_census.in_set(bevy::render::RenderSystems::Cleanup));
        }
    }
}

/// Every [`CENSUS_SECS`], `stress render: gpu images <n>`: the images the render world
/// holds, to set against the main world's `images` and the GPU allocator's textures
/// (issue #903: allocator textures doubling over a tour while `images` stayed flat).
fn render_census(
    images: Res<bevy::render::render_asset::RenderAssets<bevy::render::texture::GpuImage>>,
    allocators: Option<Res<bevy::render::material_bind_groups::MaterialBindGroupAllocators>>,
    mut last: Local<Option<std::time::Instant>>,
) {
    // The render world has no clock of its own: real time.
    let now = std::time::Instant::now();
    if last.is_some_and(|t| now.duration_since(t).as_secs_f32() < CENSUS_SECS) {
        return;
    }
    *last = Some(now);
    let mut by_label: HashMap<&str, usize> = HashMap::new();
    for (_, g) in images.iter() {
        *by_label.entry(g.texture_descriptor.label.unwrap_or("(no label)")).or_default() += 1;
    }
    let mut labels: Vec<_> = by_label.into_iter().collect();
    labels.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    let listed: Vec<String> = labels.iter().map(|(l, n)| format!("{l} {n}")).collect();
    info!("stress render: gpu images {}; {}", images.iter().count(), listed.join("; "));
    if let Some(a) = allocators.as_deref().and_then(|a| a.get(&std::any::TypeId::of::<nvf_assets::NifMaterial>())) {
        info!("stress render: nif material slabs {}, resources in them {}", a.slab_count(), a.allocations());
    }
}

/// The caches outside the release sweep (issue #904), for [`cache_census`].
#[derive(bevy::ecs::system::SystemParam)]
struct Caches<'w> {
    materials: Res<'w, Assets<nvf_assets::NifMaterial>>,
    images: Res<'w, Assets<Image>>,
    models: Option<Res<'w, nvf_assets::ModelCache>>,
    textures: Option<Res<'w, nvf_assets::TextureCache>>,
    emittance: Option<Res<'w, crate::emittance::EmittanceState>>,
    faded: Option<Res<'w, crate::effect_shaders::Shaders>>,
    swapped: Option<Res<'w, crate::texture_swap::Swapped>>,
    particles: Option<Res<'w, crate::particles::Particles>>,
    terrain: Option<Res<'w, crate::terrain::TerrainCache>>,
    grass: Option<Res<'w, crate::grass::GrassCache>>,
    trees: Option<Res<'w, crate::tree::TreeCache>>,
    water: Option<Res<'w, crate::water::WaterCache>>,
    sfx: Option<Res<'w, nvf_play::sfx::Sfx>>,
    pcm: Option<Res<'w, Assets<nvf_play::sfx::PcmSound>>>,
    radio: Option<Res<'w, nvf_play::radio::Radio>>,
    music: Option<Res<'w, nvf_play::music::Music>>,
    ambient: Option<Res<'w, nvf_play::ambient::Ambient>>,
}

/// Every [`CENSUS_SECS`] of real time, `stress caches: <cache> <entries> (<idle> idle, <n>
/// images <MB> MB); …`: what the caches the release sweep does not see hold, an entry
/// idle when nothing outside its cache holds it, and the images its materials sample
/// with their GPU bytes (an image two caches share counts in both).
fn cache_census(real: Res<Time<Real>>, c: Caches, mut last: Local<f32>) {
    let now = real.elapsed_secs();
    if now - *last < CENSUS_SECS {
        return;
    }
    *last = now;
    let m = &*c.materials;
    let mut listed: Vec<String> = Vec::new();
    let mut put = |name: &str, s: Option<nvf_assets::release::CacheShare>| {
        if let Some(s) = s {
            listed.push(format!("{name} {}", s.text(&c.images)));
        }
    };
    put("textures", c.textures.as_deref().map(nvf_assets::TextureCache::share));
    put("emittance copies", c.emittance.as_deref().map(|e| e.share(m)));
    put("see-through copies", c.faded.as_deref().map(|e| e.share(m)));
    put("swapped copies", c.swapped.as_deref().map(|e| e.share(m)));
    put("particle files", c.models.as_deref().map(|e| e.particle_share(m)));
    if let Some([placed, shared]) = c.particles.as_deref().map(|p| p.share(m)) {
        put("placed particle systems", Some(placed));
        put("shared particle systems", Some(shared));
    }
    put("terrain materials", c.terrain.as_deref().map(|e| e.share(m)));
    let sounds = match (c.sfx.as_deref(), c.pcm.as_deref()) {
        (Some(s), Some(p)) => {
            let (n, idle, bytes) = s.file_share(p);
            format!("; sound files {n} ({idle} idle, {:.0} MB, {} let go)", bytes as f64 / (1024.0 * 1024.0), s.files_released)
        }
        _ => String::new(),
    };
    let streamed = [
        ("radio files", c.radio.as_deref().map(|r| &r.files)),
        ("music files", c.music.as_deref().map(|m| &m.files)),
        ("ambient files", c.ambient.as_deref().map(|a| &a.files)),
    ];
    let mut sounds = sounds;
    for (name, f) in streamed {
        if let Some(f) = f {
            let (n, idle, bytes) = f.share();
            sounds.push_str(&format!("; {name} {n} ({idle} idle, {:.0} MB, {} let go)", bytes as f64 / (1024.0 * 1024.0), f.released));
        }
    }
    let counts = format!(
        "grass models {}; trees {}; water materials {}{sounds}",
        c.grass.as_deref().map_or(0, crate::grass::GrassCache::models),
        c.trees.as_deref().map_or(0, crate::tree::TreeCache::len),
        c.water.as_deref().map_or(0, crate::water::WaterCache::len)
    );
    info!("stress caches: {}; {counts}", listed.join("; "));
}

/// Seconds between entity censuses ([`census`]).
const CENSUS_SECS: f32 = 30.0;

/// Kinds listed in a census line.
const CENSUS_LISTED: usize = 14;

/// Every [`CENSUS_SECS`] of real time, `stress census: <n> entities; <count> <kind>; …`: the
/// entities by kind, the most first, so an entity count that grows can be told by what
/// grows [observed 2026-10-27: a two-lap soak ended its second lap with 9,900 entities
/// against 1,907, at the same place 9,886 against 1,754]. An entity's kind is the names of
/// the game's own components on it (`nvforever`, `nvf_*`, `fnv_*`), else of Bevy's, the
/// first four, with `(root)` when it has no parent.
fn census(world: &World, mut last: Local<f32>) {
    let now = world.resource::<Time<Real>>().elapsed_secs();
    if now - *last < CENSUS_SECS {
        return;
    }
    *last = now;
    let components = world.components();
    let parent = world.component_id::<ChildOf>();
    let mut kinds: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut total = 0usize;
    for a in world.archetypes().iter() {
        let n = a.len() as usize;
        if n == 0 {
            continue;
        }
        total += n;
        let names: Vec<String> = a.components().iter().filter_map(|id| components.get_name(*id)).map(|n| n.to_string()).collect();
        let ours: Vec<&String> = names.iter().filter(|n| n.starts_with("nvforever") || n.starts_with("nvf_") || n.starts_with("fnv_")).collect();
        let pick: Vec<&String> = if ours.is_empty() { names.iter().collect() } else { ours };
        let mut short: Vec<String> = pick.iter().map(|n| crate::profile::short_name(n)).collect();
        short.sort();
        short.truncate(4);
        let rooted = parent.is_some_and(|p| a.contains(p));
        let key = format!("{}{}", short.join("+"), if rooted { "" } else { " (root)" });
        *kinds.entry(key).or_default() += n;
    }
    let mut list: Vec<(String, usize)> = kinds.into_iter().collect();
    list.sort_by_key(|k| std::cmp::Reverse(k.1));
    let shown: Vec<String> = list.iter().take(CENSUS_LISTED).map(|(k, n)| format!("{n} {k}")).collect();
    info!("stress census: {total} entities; {}", shown.join("; "));
}

/// A frame slower than this (milliseconds) after start-up is logged as a hitch.
pub const HITCH_MS: f32 = 250.0;

#[derive(Resource, Default)]
struct Samples {
    every: f32,
    since: f32,
    /// The start's cells are in: frame times count from the next frame on.
    started: bool,
    /// Start-up's frame times (milliseconds).
    startup: Vec<f32>,
    /// The streamer's cells loaded and their load time so far, at the last frame.
    cells_loaded: usize,
    load_ms: f64,
    /// Actors drawn and their time so far, at the last frame (`readahead::clock`).
    actors_drawn: usize,
    actors_ms: f64,
    /// After start-up, the frames that drew actors: (frame ms, actors, ms drawing them).
    actor_frames: Vec<(f32, usize, f64)>,
    /// This window's frame times, and every frame's (milliseconds).
    window: Vec<f32>,
    all: Vec<f32>,
    /// After start-up, frames behind a loading screen (left out of the frame times).
    hidden: usize,
}

/// (mean, 1 % low, worst) of frame times in milliseconds: the 1 % low is the mean of the
/// slowest hundredth of the frames (one frame at least). Zeros with no frames.
pub fn frame_stats(ms: &[f32]) -> (f32, f32, f32) {
    if ms.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mean = ms.iter().sum::<f32>() / ms.len() as f32;
    let mut sorted = ms.to_vec();
    sorted.sort_by(|a, b| b.total_cmp(a));
    let n = (ms.len() / 100).max(1);
    let low = sorted[..n].iter().sum::<f32>() / n as f32;
    (mean, low, sorted[0])
}

/// The caches a sample counts, and the device whose allocator it reports.
type Cached<'w> = (Option<Res<'w, nvf_assets::ModelCache>>, Option<Res<'w, nvf_assets::TextureCache>>, Option<Res<'w, bevy::render::renderer::RenderDevice>>);

#[allow(clippy::too_many_arguments)]
fn sample(
    real: Res<Time<Real>>,
    mut s: ResMut<Samples>,
    entities: Query<Entity>,
    refs: Query<(), With<RefVisual>>,
    actors: Query<&ActorRig>,
    globals: Query<&GlobalTransform>,
    audio: Query<(), With<AudioSink>>,
    meshes: Res<Assets<Mesh>>,
    images: Res<Assets<Image>>,
    materials: Res<Assets<nvf_assets::NifMaterial>>,
    cached: Cached,
    phys: Option<Res<PhysState>>,
    combat: Option<Res<nvf_play::combat::Combat>>,
    streamer: Option<Res<crate::world::Streamer>>,
    loading: Option<Res<nvf_play::loading::Loading>>,
) {
    let dt = real.delta_secs();
    let ms = dt * 1000.0;
    s.since += dt;
    let (cells_loaded, load_ms) = streamer.as_deref().map_or((0, 0.0), |st| (st.stats.cells_loaded, st.stats.load_ms));
    let (new_cells, new_ms) = (cells_loaded.saturating_sub(s.cells_loaded), load_ms - s.load_ms);
    (s.cells_loaded, s.load_ms) = (cells_loaded, load_ms);
    let (actors_drawn, actors_ms) = crate::readahead::clock();
    let (new_actors, new_actors_ms) = (actors_drawn - s.actors_drawn, actors_ms - s.actors_ms);
    (s.actors_drawn, s.actors_ms) = (actors_drawn, actors_ms);
    if !s.started {
        s.startup.push(ms);
        // A start that never settles still ends start-up after a minute.
        if streamer.as_deref().is_none_or(|st| st.settled) || s.startup.iter().sum::<f32>() > 60_000.0 {
            s.started = true;
            let total: f32 = s.startup.iter().sum();
            let slowest = s.startup.iter().copied().fold(0.0, f32::max);
            info!("stress start-up: {} frames, {total:.0} ms, slowest {slowest:.2} ms", s.startup.len());
        }
    } else if loading.as_deref().is_some_and(nvf_play::loading::Loading::unseen_frames) {
        // Behind a loading screen the player sees no frame: not a hitch, not a frame time.
        s.hidden += 1;
        if ms > HITCH_MS {
            info!("stress hitch (behind a loading screen): {ms:.0} ms at t {:.1} s, {new_cells} cells loaded in it ({new_ms:.0} ms), at {}", real.elapsed_secs(), crate::crash::place_text());
        }
    } else {
        s.window.push(ms);
        s.all.push(ms);
        if new_actors > 0 {
            s.actor_frames.push((ms, new_actors, new_actors_ms));
        }
        if ms > HITCH_MS {
            info!(
                "stress hitch: {ms:.0} ms at t {:.1} s, {new_cells} cells loaded in it ({new_ms:.0} ms), {new_actors} actors drawn in it ({new_actors_ms:.0} ms), at {}",
                real.elapsed_secs(),
                crate::crash::place_text()
            );
        }
    }
    if s.since < s.every {
        return;
    }
    s.since = 0.0;
    let (mean, _, worst) = frame_stats(&s.window);
    let frames = s.window.len();
    s.window.clear();
    let nan = actors.iter().filter(|r| r.nodes.first().and_then(|&e| globals.get(e).ok()).is_some_and(|g| !g.translation().is_finite())).count();
    let (cells, bodies, ragdolls) = phys.as_deref().map_or((0, "0/0/0".to_string(), 0), |p| {
        let st = p.world.stats();
        let solver = format!(", bodies awake {}, contact pairs {}, manifolds {}, solver contacts {}, joints {}, solver ms {:.0}/{:.0}/{:.0}/{:.0}/{:.0}/{:.0}", st.bodies_awake, st.contact_pairs, st.manifolds, st.solver_contacts, st.joints, st.solver_us[0] as f64 / 1000.0, st.solver_us[1] as f64 / 1000.0, st.solver_us[2] as f64 / 1000.0, st.solver_us[3] as f64 / 1000.0, st.solver_us[4] as f64 / 1000.0, st.solver_us[5] as f64 / 1000.0);
        (p.cells.len(), format!("{}/{}/{}{solver}", st.bodies_fixed, st.bodies_kinematic, st.bodies_dynamic), p.world.ragdolls().count())
    });
    let gpu = cached.2.as_deref().map_or(String::new(), gpu_memory);
    info!(
        "stress sample: t {:.1} s, frames {frames}, frame mean {mean:.2} ms, frame worst {worst:.2} ms, entities {}, refs drawn {}, meshes {}, images {}, materials {}, models cached {}, textures cached {}, audio {}, cells {cells}, bodies {bodies}, actors {}, nan actors {nan}, ragdolls {ragdolls}, projectiles {}{gpu}, at {}",
        real.elapsed_secs(),
        entities.iter().count(),
        refs.iter().count(),
        meshes.len(),
        images.len(),
        materials.len(),
        cached.0.as_deref().map_or(0, nvf_assets::ModelCache::held),
        cached.1.as_deref().map_or(0, nvf_assets::TextureCache::len),
        audio.iter().count(),
        actors.iter().count(),
        combat.as_deref().map_or(0, |c| c.in_flight()),
        crate::crash::place_text(),
    );
}

/// The GPU allocator's totals as sample fields (`, gpu allocated <MB>, gpu reserved <MB>,
/// gpu blocks <n>`), and its allocations by label, largest first, on a line of their own:
/// reserved beyond allocated is room in blocks kept, allocated growing with the asset
/// counts flat is something still holding GPU memory (issue #903). Empty where the
/// backend gives no report.
fn gpu_memory(device: &bevy::render::renderer::RenderDevice) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let Some(r) = device.wgpu_device().generate_allocator_report() else { return String::new() };
    let mut by_label: HashMap<&str, (u64, usize)> = HashMap::new();
    for a in &r.allocations {
        let e = by_label.entry(if a.name.is_empty() { "(no label)" } else { &a.name }).or_default();
        e.0 += a.size;
        e.1 += 1;
    }
    let mut labels: Vec<_> = by_label.into_iter().collect();
    labels.sort_by_key(|&(_, (bytes, _))| std::cmp::Reverse(bytes));
    let listed: Vec<String> = labels.iter().take(40).map(|(l, (b, n))| format!("{l} {:.0} MB in {n}", *b as f64 / MB)).collect();
    info!("stress gpu labels: {}", listed.join("; "));
    format!(
        ", gpu allocated {:.0}, gpu reserved {:.0}, gpu blocks {}",
        r.total_allocated_bytes as f64 / MB,
        r.total_reserved_bytes as f64 / MB,
        r.blocks.len()
    )
}

fn report(mut exit: MessageReader<AppExit>, s: Res<Samples>) {
    if exit.read().next().is_none() {
        return;
    }
    let (mean, low, worst) = frame_stats(&s.all);
    let fps = if low > 0.0 { 1000.0 / low } else { 0.0 };
    info!("stress frames: {} frames, mean {mean:.2} ms, 1% low {low:.2} ms ({fps:.1} fps), worst {worst:.2} ms; {} behind loading screens left out", s.all.len(), s.hidden);
    let f = &s.actor_frames;
    let (drawn, ms) = f.iter().fold((0, 0.0), |(n, t), &(_, k, m)| (n + k, t + m));
    let most = f.iter().map(|x| x.2).fold(0.0, f64::max);
    let frame_ms: Vec<f32> = f.iter().map(|x| x.0).collect();
    let (mean, _, worst) = frame_stats(&frame_ms);
    let slow = frame_ms.iter().filter(|&&m| m > 100.0).count();
    info!(
        "stress actors: {drawn} drawn in {} frames ({ms:.0} ms, most in a frame {most:.1} ms); those frames mean {mean:.1} ms, worst {worst:.0} ms, {slow} over 100 ms",
        f.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_stats_take_the_slowest_hundredth_as_the_low() {
        assert_eq!(frame_stats(&[]), (0.0, 0.0, 0.0));
        // 198 frames of 10 ms and two of 50 ms: the slowest 1 % (2 frames) are the 50s.
        let mut ms = vec![10.0; 198];
        ms.extend([50.0, 50.0]);
        let (mean, low, worst) = frame_stats(&ms);
        assert!((mean - 10.4).abs() < 1e-4, "{mean}");
        assert_eq!((low, worst), (50.0, 50.0));
        // Fewer than 100 frames: the worst one.
        assert_eq!(frame_stats(&[5.0, 20.0, 8.0]).1, 20.0);
    }
}
