//! The frame split row's times: what a frame took on the main thread, on the render
//! thread and on the GPU, so a frame held up by the CPU can be told from one held up by
//! the GPU (issue #1868). Each is the mean over the readout's window
//! (`readings::WINDOW`) of what each frame took [design]:
//!
//! - **main**: the main schedule's run, from the start of `First` (before the clocks'
//!   update, `bevy::time::TimeSystems`) to `Last`, before the frame rate limit's wait
//!   (`frame_pace::cap_rate`): the game's own systems. It leaves out the wait for the
//!   render thread at extraction and the window events, which come between two runs of
//!   the schedule [design: bevy_render 0.20.1 `pipelined_rendering.rs`, the main app
//!   hands its world to the render thread after `Last`]; so `main` is what the main
//!   thread worked, not the frame's time.
//! - **render**: the render world's whole `Render` schedule, from before its first system
//!   set (`RenderSystems::ExtractCommands`) to after its last (`RenderSystems::PostCleanup`),
//!   on the render thread (pipelined rendering). It includes the wait for the window's
//!   next image (`prepare_windows`, vendor/bevy_render `view/window/mod.rs`: its
//!   `get_current_texture` can wait long while the GPU is behind), which is shown on its
//!   own as **wait**: the span from just before `prepare_windows` to just after it. Other
//!   prepare systems run beside `prepare_windows`, so render less the wait would
//!   understate the render thread's work; the row shows both instead, and with vsync the
//!   wait is the display's time. GPU time is what tells a GPU-bound frame.
//! - **GPU**: the sum of the top-level render passes' GPU times from GPU timestamp
//!   queries, as Bevy's render diagnostics give them (`RenderDiagnosticsPlugin`,
//!   `readings::gpu_frame_ms`), each the mean of its measurements of the frames recorded
//!   in the reading's half second and since the row began to show ([`mean_where`],
//!   [`GpuFrames`]), so an earlier showing's are never in it. Passes Bevy does not
//!   time (the shadow passes, `graphics.rs`) are not in it, so it is a lower bound of the
//!   GPU's frame. The plugin can only be added when the app is built: it is added at the
//!   start when the INI, applied as the settings will apply it, has the split row and the
//!   overlay on ([`timestamps_wanted`]); a row switched on later shows the GPU part from
//!   the next start. Without timestamp queries on the device, or without the plugin, the
//!   GPU part is left out.
//!
//! Timestamps cost a little GPU time and a read-back each frame. When the overlay alone
//! added the plugin, its recorder (`DiagnosticsRecorder`) is taken out of the render world
//! while the row is not shown ([`gate_recorder`]): the passes record nothing without it
//! (`RenderContext::diagnostic_recorder` is optional) and the plugin's own systems do
//! nothing (vendor/README.md, bevy_render's recorder gate). When a measuring path asked
//! for the diagnostics too (`graphics::MeasuringDiagnostics`: the frame-time aid, frame
//! telemetry's `gpu`, the span profile; or Bevy added them itself, with Tracy) the
//! recorder stays [design].
//!
//! While the row is off (or the overlay hidden) the stamping systems return at once:
//! nothing is timed or kept. Each showing of the row has a generation ([`Clock::set_on`]):
//! a span is tagged with the generation it began in and is dropped when it ends in
//! another, so a span under way across a hide and a show is not in the new showing, as
//! `usage.rs` does for the sampler's collections.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::diagnostic::DiagnosticsStore;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::diagnostic::{DiagnosticsRecorder, RenderDiagnosticsPlugin, RenderDiagnosticsRecorded};
use bevy::render::renderer::RenderDevice;
use bevy::render::{Render, RenderApp, RenderSystems};
use nvf_play::settings::{switch_after_ini, GivenSwitches};

use super::readings::{gpu_frame_ms, Rows, Split, WINDOW};

// One accumulator in one `AtomicU64`: the generation of the showing it counts for (8
// bits), the frames (20 bits) and their microseconds (36 bits). A frame is added and the
// window taken each in a single atomic step, so a frame's count and its time are always in
// the same window [design].
//
// Overflow: the window is taken every half second while the row shows
// (`super::sample`, `readings::WINDOW`); the spans of one accumulator follow each other on
// one thread, so its microseconds stay under the wall time between two takes (2^36 µs is
// 19 hours), and its frames under the frames drawn meanwhile (2^20 is over a million). The
// add saturates both fields all the same, so neither can carry into the next. The
// generation wraps after 256 showings; a span lasts one frame, and a showing at most one
// change of the switches a frame (`super::switch`), so a span never sees its generation
// come round again.
const US_BITS: u32 = 36;
const COUNT_BITS: u32 = 20;
const US_MAX: u64 = (1 << US_BITS) - 1;
const COUNT_MAX: u64 = (1 << COUNT_BITS) - 1;
const GEN_SHIFT: u32 = US_BITS + COUNT_BITS;
const GEN_MASK: u64 = !((1 << GEN_SHIFT) - 1);

fn pack(generation: u8, frames: u64, us: u64) -> u64 {
    (u64::from(generation) << GEN_SHIFT) | (frames.min(COUNT_MAX) << US_BITS) | us.min(US_MAX)
}

fn unpack(w: u64) -> (u8, u64, u64) {
    ((w >> GEN_SHIFT) as u8, (w >> US_BITS) & COUNT_MAX, w & US_MAX)
}

/// Frames and their time since the last take, for one showing ([`Clock`]).
#[derive(Default)]
struct Acc(AtomicU64);

impl Acc {
    /// Add a frame that took `took`, if the accumulator still counts for `generation`
    /// (else the frame began in an earlier showing and is dropped). One compare-and-swap
    /// step: the generation check, the frame and its time go in together.
    fn add(&self, generation: u8, took: Duration) -> bool {
        let us = took.as_micros().min(u128::from(US_MAX)) as u64;
        // A compare-and-swap loop written out: `fetch_update` is deprecated on newer
        // toolchains in favour of `try_update`, which older ones do not have.
        let mut w = self.0.load(Ordering::Acquire);
        loop {
            let (g, frames, sum) = unpack(w);
            if g != generation {
                return false;
            }
            let next = pack(g, frames.saturating_add(1), sum.saturating_add(us));
            match self.0.compare_exchange_weak(w, next, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return true,
                Err(now) => w = now,
            }
        }
    }

    /// The frames and microseconds since the last take, zeroed in the same step (the
    /// generation kept).
    fn take(&self) -> (u64, u64) {
        let (_, frames, us) = unpack(self.0.fetch_and(GEN_MASK, Ordering::AcqRel));
        (frames, us)
    }

    /// The mean frame (ms) since the last take; none without a frame.
    fn take_mean_ms(&self) -> Option<f32> {
        let (frames, us) = self.take();
        (frames > 0).then(|| us as f32 / 1000.0 / frames as f32)
    }

    /// Start empty for a new showing.
    fn start(&self, generation: u8) {
        self.0.store(pack(generation, 0, 0), Ordering::Release);
    }
}

/// When a span began and the showing it began in.
#[derive(Clone, Copy, Debug)]
struct Stamp {
    at: Instant,
    generation: u8,
}

/// The means a reading takes ([`Clock::take`]), ms; none without a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Means {
    main: Option<f32>,
    render: Option<f32>,
    wait: Option<f32>,
}

/// What the main and the render thread add their frames' times to, shared by both worlds.
#[derive(Default)]
pub struct Clock {
    on: AtomicBool,
    /// The showing's generation, bumped each time the row starts to show.
    generation: AtomicU8,
    main: Acc,
    render: Acc,
    wait: Acc,
    /// When the row last started to show (read by the main world only).
    began: Mutex<Option<Instant>>,
}

impl Clock {
    /// Whether the row shows: the spans are taken, and the GPU recorder wanted
    /// ([`gate_recorder`]).
    fn is_on(&self) -> bool {
        self.on.load(Ordering::Acquire)
    }

    /// The row shows or not (the overlay shown and the row on). Turned on, it is a new
    /// showing: a new generation, every accumulator empty for it, and the GPU times read
    /// from now on.
    pub fn set_on(&self, on: bool) {
        if !on {
            self.on.store(false, Ordering::Release);
            return;
        }
        if self.is_on() {
            return;
        }
        // The generation and the empty accumulators first: a span stamped once the row
        // is on is stamped with this showing's.
        let generation = self.generation.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
        for a in [&self.main, &self.render, &self.wait] {
            a.start(generation);
        }
        if let Ok(mut b) = self.began.lock() {
            *b = Some(Instant::now());
        }
        self.on.store(true, Ordering::Release);
    }

    /// A span begins now, if the row shows.
    fn stamp(&self) -> Option<Stamp> {
        self.is_on().then(|| Stamp { at: Instant::now(), generation: self.generation.load(Ordering::Acquire) })
    }

    /// The means since the last call.
    fn take(&self) -> Means {
        Means { main: self.main.take_mean_ms(), render: self.render.take_mean_ms(), wait: self.wait.take_mean_ms() }
    }

    /// The current showing's generation.
    fn generation(&self) -> u8 {
        self.generation.load(Ordering::Acquire)
    }

    /// When the row last started to show.
    fn began(&self) -> Option<Instant> {
        self.began.lock().ok().and_then(|b| *b)
    }
}

/// The shared [`Clock`], in the main world and the render world.
#[derive(Resource, Clone, Default)]
pub struct SplitClock(pub Arc<Clock>);

/// When this frame's main schedule began, while the row shows.
#[derive(Resource, Default)]
struct MainSpan(Option<Stamp>);

/// In the render world: when its schedule began and when the wait for the window's image
/// did.
#[derive(Resource, Default)]
struct RenderSpan {
    began: Option<Stamp>,
    wait_began: Option<Instant>,
}

/// Whether Bevy's render diagnostics were added when the app was built (by this row at
/// the start, or by a measuring aid), set once every plugin is built.
#[derive(Resource, Clone, Copy, Default)]
pub struct GpuTimes {
    pub diagnostics: bool,
}

/// Whether this row's start added the render diagnostics itself
/// (`graphics::add_render_diagnostics`), so their recorder may be taken out while the row
/// is hidden.
#[derive(Resource, Clone, Copy, Default)]
struct OverlayAddedDiagnostics(bool);

/// Whether GPU timestamps are taken from the start: the overlay and the split row on
/// once the INI is applied, by the settings' own precedence
/// (`nvf_play::settings::switch_after_ini`, as `apply_stored` decides): a stored state
/// wins over the command line, except where a test run keeps a switch it was given
/// (`--fps`, named in the Frame rate counter's switches). `fps`: `--fps`; `counter`, `split`:
/// the INI's states, `None` where it has none; `settings`: the switches this run was
/// given, `None` when the run has no settings page (not `--play`: the INI is not applied
/// and the split row stays off).
pub fn timestamps_wanted(fps: bool, counter: Option<bool>, split: Option<bool>, settings: Option<&GivenSwitches>) -> bool {
    let Some(given) = settings else { return false };
    let show = switch_after_ini(fps, counter, given, crate::settings::FPS_COUNTER_SWITCHES);
    let split = switch_after_ini(Rows::default().split, split, given, crate::settings::FPS_SPLIT_SWITCHES);
    show && split
}

/// [`timestamps_wanted`] from the INI this run reads (`settings::stored_at_start`).
pub fn timestamps_at_start(fps: bool, settings: Option<&GivenSwitches>) -> bool {
    let stored = |key: &str| crate::settings::stored_at_start(key).and_then(|v| nvf_play::settings::parse_switch(&v));
    timestamps_wanted(fps, stored("fps-counter"), stored("fps-counter-split"), settings)
}

/// The clock and its stamping systems in both worlds; the render diagnostics when
/// `timestamps` (the row on at the start).
pub fn build(app: &mut App, timestamps: bool) {
    let mut added = false;
    if timestamps {
        info!("fps counter: the frame split row is on: GPU timestamps are taken for its GPU time");
        added = crate::graphics::add_render_diagnostics(app);
    }
    let clock = SplitClock::default();
    app.insert_resource(clock.clone())
        .insert_resource(OverlayAddedDiagnostics(added))
        .init_resource::<MainSpan>()
        .init_resource::<GpuFrames>()
        .add_systems(First, main_begin.before(bevy::time::TimeSystems))
        .add_systems(Last, main_end.before(crate::frame_pace::cap_rate));
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .insert_resource(clock)
            .init_resource::<RenderSpan>()
            .add_systems(Render, render_begin.before(RenderSystems::ExtractCommands))
            .add_systems(Render, (wait_begin.before(bevy::render::view::prepare_windows), wait_end.after(bevy::render::view::prepare_windows)).in_set(RenderSystems::PrepareViews))
            .add_systems(Render, render_end.after(RenderSystems::PostCleanup));
    }
}

/// Whether the recorder is taken out while the row is hidden: the overlay alone added the
/// render diagnostics, and no measuring path asked for them (in any order of the plugins).
fn gates_recorder(app: &App) -> bool {
    app.world().get_resource::<OverlayAddedDiagnostics>().is_some_and(|a| a.0) && !app.world().contains_resource::<crate::graphics::MeasuringDiagnostics>()
}

/// Once every plugin is built: whether the render diagnostics are there, and whether the
/// recorder follows the row ([`gate_recorder`]).
pub fn finish(app: &mut App) {
    let diagnostics = app.is_plugin_added::<RenderDiagnosticsPlugin>();
    app.insert_resource(GpuTimes { diagnostics });
    if !diagnostics {
        return;
    }
    if !gates_recorder(app) {
        info!("fps counter: GPU timestamps are taken all the time (the render diagnostics are also a measuring path's)");
        return;
    }
    info!("fps counter: GPU timestamps are taken only while the frame split shows");
    if let Some(render) = app.get_sub_app_mut(RenderApp) {
        render
            .init_resource::<TakenOut>()
            .init_resource::<RecorderShowing>()
            .add_systems(Render, gate_recorder.after(RenderSystems::ExtractCommands).before(bevy::render::diagnostic::begin_diagnostics_frame));
    }
}

/// In the render world: the recorder last taken out, kept until the next one is. Dropping
/// it discards any read-back it still had under way, which only loses measurements of
/// frames before the row showed again [design].
#[derive(Resource, Default)]
struct TakenOut(Option<DiagnosticsRecorder>);

/// In the render world: the showing the recorder in the world records for; none before the
/// gate first saw one.
#[derive(Resource, Default)]
struct RecorderShowing(Option<u8>);

/// What [`gate_recorder`] does with the recorder.
#[derive(Debug, PartialEq, Eq)]
enum Gate {
    Keep,
    TakeOut,
    /// Put a new one in (taking out the one there, if any).
    Renew,
}

/// The gate's decision: `wanted` whether the row shows, `present` whether a recorder is in
/// the world, `made` whether the gate has taken one out before (Bevy has made its own),
/// `records_for` the showing the present recorder records for, `showing` the current one.
/// A hide and a show between two render frames leave the row shown at both, so the
/// showing's generation, not only whether it shows, tells a new showing [design].
fn gate(wanted: bool, present: bool, made: bool, records_for: Option<u8>, showing: u8) -> Gate {
    match (wanted, present) {
        (false, true) => Gate::TakeOut,
        (true, false) if made => Gate::Renew,
        (true, true) if records_for.is_some_and(|g| g != showing) => Gate::Renew,
        _ => Gate::Keep,
    }
}

/// Before the recorder begins a frame: while the row is not shown, take the recorder out
/// of the render world (no pass records, nothing is resolved or read back); when a new
/// showing begins, put a new one in, so no frame recorded before it is read back into it.
/// Nothing before Bevy made the first one (`RenderStartup`); after a lost device Bevy
/// makes it again, and it is taken out again while the row is hidden.
fn gate_recorder(world: &mut World) {
    let clock = world.resource::<SplitClock>().0.clone();
    let (wanted, showing) = (clock.is_on(), clock.generation());
    let present = world.contains_resource::<DiagnosticsRecorder>();
    let made = world.resource::<TakenOut>().0.is_some();
    let records_for = world.resource::<RecorderShowing>().0;
    match gate(wanted, present, made, records_for, showing) {
        Gate::TakeOut => {
            let recorder = world.remove_resource::<DiagnosticsRecorder>();
            world.resource_mut::<TakenOut>().0 = recorder;
        }
        Gate::Renew => {
            if let Some(old) = world.remove_resource::<DiagnosticsRecorder>() {
                world.resource_mut::<TakenOut>().0 = Some(old);
            }
            let fresh = DiagnosticsRecorder::from_world(world);
            world.insert_resource(fresh);
            world.resource_mut::<RecorderShowing>().0 = Some(showing);
        }
        Gate::Keep => {
            if wanted && present && records_for.is_none() {
                world.resource_mut::<RecorderShowing>().0 = Some(showing);
            }
        }
    }
}

fn main_begin(clock: Res<SplitClock>, mut span: ResMut<MainSpan>) {
    span.0 = clock.0.stamp();
}

fn main_end(clock: Res<SplitClock>, mut span: ResMut<MainSpan>) {
    if let Some(s) = span.0.take() {
        clock.0.main.add(s.generation, s.at.elapsed());
    }
}

fn render_begin(clock: Res<SplitClock>, mut span: ResMut<RenderSpan>) {
    *span = RenderSpan { began: clock.0.stamp(), wait_began: None };
}

fn wait_begin(mut span: ResMut<RenderSpan>) {
    if span.began.is_some() {
        span.wait_began = Some(Instant::now());
    }
}

fn wait_end(clock: Res<SplitClock>, mut span: ResMut<RenderSpan>) {
    if let (Some(s), Some(at)) = (span.began, span.wait_began.take()) {
        clock.0.wait.add(s.generation, at.elapsed());
    }
}

fn render_end(clock: Res<SplitClock>, mut span: ResMut<RenderSpan>) {
    if let Some(s) = span.began.take() {
        clock.0.render.add(s.generation, s.at.elapsed());
    }
}

/// The mean of a render pass's measurements `(when, value)` whose time `keep` takes, and
/// the seconds from the last of them to `now`; none without one.
pub fn mean_where(measurements: impl IntoIterator<Item = (Instant, f64)>, keep: impl Fn(Instant) -> bool, now: Instant) -> Option<(f64, f64)> {
    let (mut sum, mut n, mut last) = (0.0, 0u32, None::<Instant>);
    for (at, v) in measurements {
        if !keep(at) || !v.is_finite() {
            continue;
        }
        sum += v;
        n += 1;
        last = Some(last.map_or(at, |l| l.max(at)));
    }
    let last = last?;
    Some((sum / f64::from(n), now.saturating_duration_since(last).as_secs_f64()))
}

/// The render diagnostics' frames put in the store while the row shows, oldest first: when
/// each was put in (the time its measurements carry) and when it was recorded
/// (`RenderDiagnosticsRecorded`). Kept for [`GPU_FRAMES_KEPT`].
#[derive(Resource, Default)]
pub struct GpuFrames(std::collections::VecDeque<(Instant, Instant)>);

/// How long [`GpuFrames`] keeps a frame: twice the reading's window, so a reading finds the
/// frames of its window however late they came [design].
const GPU_FRAMES_KEPT: Duration = Duration::from_secs(1);

impl GpuFrames {
    /// When the frame whose measurements the store stamped `imported` was recorded; none
    /// for a frame not noted (put in before the row showed). Each measurement is looked up
    /// on its own: the recorder can hand its frames over out of order (it takes finished
    /// frames with `swap_remove`), so the import order is not the recording order.
    pub fn recorded_for(&self, imported: Instant) -> Option<Instant> {
        self.0.iter().rev().find(|(at, _)| *at == imported).map(|(_, recorded)| *recorded)
    }

    fn note(&mut self, imported: Instant, recorded: Instant, now: Instant) {
        if self.0.back().is_none_or(|(last, _)| *last != imported) {
            self.0.push_back((imported, recorded));
        }
        while self.0.front().is_some_and(|(at, _)| now.saturating_duration_since(*at) > GPU_FRAMES_KEPT) {
            self.0.pop_front();
        }
    }
}

/// Notes the frame Bevy last put in the store, while the row shows; forgets them while not.
/// After `PreUpdate` (where Bevy puts the render diagnostics in the store) and before the
/// overlay's reading (`super::sample`), in its chain.
pub(super) fn note_gpu_frame(clock: Res<SplitClock>, recorded: Option<Res<RenderDiagnosticsRecorded>>, mut frames: ResMut<GpuFrames>) {
    if !clock.0.is_on() {
        if !frames.0.is_empty() {
            frames.0.clear();
        }
        return;
    }
    if let Some((imported, began)) = recorded.and_then(|r| r.0) {
        frames.note(imported, began, Instant::now());
    }
}

/// What a reading of the split row needs.
#[derive(SystemParam)]
pub struct SplitInputs<'w> {
    clock: Res<'w, SplitClock>,
    store: Option<Res<'w, DiagnosticsStore>>,
    gpu: Option<Res<'w, GpuTimes>>,
    frames: Res<'w, GpuFrames>,
    device: Option<Res<'w, RenderDevice>>,
}

impl SplitInputs<'_> {
    /// The window's split: none until the main schedule ran a frame with the row on.
    pub fn read(&self) -> Option<Split> {
        let m = self.clock.0.take();
        let now = Instant::now();
        // The frames of this window recorded since the showing began: the store stamps a
        // measurement when it is put in, a few frames after its frame, so each is kept by
        // its frame's recording time (the vendored `bevy_render` diagnostic
        // `RenderDiagnosticsRecorded`, `NVForever recorded instant`; [`GpuFrames`]). The
        // GPU time then covers the window main and render do [design].
        let window = Duration::from_secs_f32(WINDOW);
        let cut = self.clock.0.began().map(|b| b.max(now.checked_sub(window).unwrap_or(b)));
        let frames = &*self.frames;
        let gpu_ms = self.store.as_deref().zip(cut).and_then(|(store, cut)| {
            let keep = |imported: Instant| frames.recorded_for(imported).is_some_and(|r| r >= cut);
            gpu_frame_ms(store.iter().filter_map(|d| {
                let (mean, age) = mean_where(d.measurements().map(|m| (m.time, m.value)), keep, now)?;
                Some((d.path().as_str(), mean, age))
            }))
        });
        Some(Split { main_ms: m.main?, render_ms: m.render, wait_ms: m.render.and(m.wait), gpu_ms })
    }

    /// Whether the device offers timestamp queries; none before there is a device.
    fn timestamps(&self) -> Option<bool> {
        self.device.as_deref().map(|d| d.features().contains(wgpu::Features::TIMESTAMP_QUERY))
    }

    fn diagnostics(&self) -> bool {
        self.gpu.as_deref().is_some_and(|g| g.diagnostics)
    }

    /// Whether GPU times are to come: the render diagnostics were added and the device
    /// may take timestamps.
    pub fn gpu_expected(&self) -> bool {
        self.diagnostics() && self.timestamps() != Some(false)
    }

    /// Why a split has no GPU time, for the log.
    pub fn no_gpu_reason(&self) -> &'static str {
        match (self.diagnostics(), self.timestamps()) {
            (_, Some(false)) => "the graphics device offers no timestamp queries",
            (false, _) => "GPU timestamps were not taken from the start (the row was off then; they are from the next start)",
            (true, _) => "no render pass has had a GPU time yet",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_frames_give_each_import_its_recording_time() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut f = GpuFrames::default();
        // Frames recorded at 16, 0 (handed over out of order) and 33 ms, put in the store
        // at 50, 66 and 83 ms.
        for (imported, recorded) in [(50, 16), (66, 0), (83, 33)] {
            f.note(at(imported), at(recorded), at(imported));
        }
        // The same import noted twice is one frame.
        f.note(at(83), at(33), at(83));
        assert_eq!(f.0.len(), 3);
        assert_eq!(f.recorded_for(at(66)), Some(at(0)));
        assert_eq!(f.recorded_for(at(40)), None, "an import not noted");
        // A cut at 10 ms keeps the frames recorded at 16 and 33, not the late older one.
        let m = [(at(50), 2.0), (at(66), 100.0), (at(83), 4.0)];
        let keep = |t: Instant| f.recorded_for(t).is_some_and(|r| r >= at(10));
        assert_eq!(mean_where(m, keep, at(100)).map(|(mean, _)| mean), Some(3.0));
        // Frames put in over a second before are forgotten.
        f.note(at(2_000), at(1_950), at(2_000));
        assert_eq!(f.0.len(), 1);
    }

    #[test]
    fn the_gate_renews_the_recorder_for_each_new_showing() {
        // Hidden: taken out; hidden with none in: kept out.
        assert_eq!(gate(false, true, false, Some(1), 1), Gate::TakeOut);
        assert_eq!(gate(false, false, true, Some(1), 1), Gate::Keep);
        // Shown again after a hide the gate saw: a new one.
        assert_eq!(gate(true, false, true, Some(1), 2), Gate::Renew);
        // Hidden and shown between two render frames: the gate saw it shown both times,
        // but the showing moved on, so a new one all the same.
        assert_eq!(gate(true, true, false, Some(1), 2), Gate::Renew);
        // The same showing: kept; Bevy's first recorder is adopted, not renewed.
        assert_eq!(gate(true, true, true, Some(2), 2), Gate::Keep);
        assert_eq!(gate(true, true, false, None, 1), Gate::Keep);
        // Shown with none in and none ever taken out (before Bevy made it): nothing to do.
        assert_eq!(gate(true, false, false, None, 1), Gate::Keep);
    }

    fn given(win: bool, flags: &[&str]) -> GivenSwitches {
        GivenSwitches { given: flags.iter().map(|f| f.to_string()).collect(), win }
    }

    #[test]
    fn timestamps_follow_the_ini_as_the_settings_apply_it() {
        let ordinary = given(false, &["--play", "--fps"]);
        // The row and the overlay stored on; --fps with the row stored on.
        assert!(timestamps_wanted(false, Some(true), Some(true), Some(&ordinary)));
        assert!(timestamps_wanted(true, None, Some(true), Some(&ordinary)), "--fps and the row stored on");
        // An ordinary run: the INI's Frame rate counter switch wins over --fps.
        assert!(!timestamps_wanted(true, Some(false), Some(true), Some(&ordinary)), "the overlay stored off wins over --fps");
        assert!(!timestamps_wanted(true, Some(true), Some(false), Some(&ordinary)), "the row off");
        assert!(!timestamps_wanted(true, Some(true), None, Some(&ordinary)), "the row never stored: off, its default");
        assert!(!timestamps_wanted(false, None, Some(true), Some(&ordinary)), "the overlay off by default");
        // A measured run (--log-frame-times, --run-seconds, ...) keeps the --fps it was
        // given over the INI; without --fps the INI still applies.
        let measured = given(true, &["--play", "--fps", "--run-seconds"]);
        assert!(timestamps_wanted(true, Some(false), Some(true), Some(&measured)), "a test run keeps --fps");
        assert!(!timestamps_wanted(true, Some(true), Some(false), Some(&measured)), "the row has no switch: the INI's state");
        let measured_no_fps = given(true, &["--play", "--run-seconds"]);
        assert!(!timestamps_wanted(false, Some(false), Some(true), Some(&measured_no_fps)));
        assert!(timestamps_wanted(false, Some(true), Some(true), Some(&measured_no_fps)));
        // No settings page (not --play): the INI is not applied, the row stays off.
        assert!(!timestamps_wanted(true, Some(true), Some(true), None));
    }

    /// The same precedence as `apply_stored`'s, which the start's decision shares.
    #[test]
    fn the_start_decision_is_the_settings_precedence() {
        let switches = crate::settings::FPS_COUNTER_SWITCHES;
        for win in [false, true] {
            for flags in [&["--fps"][..], &[][..]] {
                let g = given(win, flags);
                for (cmd, stored) in [(true, Some(false)), (false, Some(true)), (true, None), (false, None)] {
                    let kept = win && !flags.is_empty();
                    let want = if kept { cmd } else { stored.unwrap_or(cmd) };
                    assert_eq!(switch_after_ini(cmd, stored, &g, switches), want, "win {win}, {flags:?}, {cmd} against {stored:?}");
                }
            }
        }
    }

    #[test]
    fn the_clock_gives_the_mean_of_the_frames_since_the_last_reading() {
        let c = Clock::default();
        assert_eq!(c.take(), Means::default());
        assert!(c.stamp().is_none(), "off: no span");
        c.set_on(true);
        let g = c.stamp().expect("on").generation;
        c.main.add(g, Duration::from_micros(9_000));
        c.main.add(g, Duration::from_micros(11_000));
        c.render.add(g, Duration::from_micros(6_300));
        c.wait.add(g, Duration::from_micros(4_000));
        assert_eq!(c.take(), Means { main: Some(10.0), render: Some(6.3), wait: Some(4.0) });
        assert_eq!(c.take(), Means::default(), "each reading starts again");
        // Off and on again: what was added meanwhile is not in the next reading.
        c.main.add(g, Duration::from_micros(50_000));
        c.set_on(false);
        assert!(!c.is_on());
        c.set_on(true);
        assert_eq!(c.take(), Means::default());
    }

    #[test]
    fn a_take_between_two_frames_keeps_each_frame_whole() {
        let c = Clock::default();
        c.set_on(true);
        let g = c.stamp().expect("on").generation;
        c.render.add(g, Duration::from_micros(3_000));
        assert_eq!(c.render.take(), (1, 3_000));
        c.render.add(g, Duration::from_micros(5_000));
        assert_eq!(c.render.take(), (1, 5_000), "the second frame and its time in the next window");
        assert_eq!(c.render.take(), (0, 0));
        // Many frames of one length added while another thread takes: every take holds
        // whole frames, its time exactly its frames' (a count taken apart from its time
        // would leave a take whose time is not its frames' time).
        const FRAME_US: u64 = 1_234;
        const FRAMES: u64 = 20_000;
        let clock = Arc::new(Clock::default());
        clock.set_on(true);
        let g = clock.stamp().expect("on").generation;
        let adder = {
            let clock = clock.clone();
            std::thread::spawn(move || {
                for _ in 0..FRAMES {
                    assert!(clock.render.add(g, Duration::from_micros(FRAME_US)));
                }
            })
        };
        let mut seen = 0;
        while !adder.is_finished() {
            let (frames, us) = clock.render.take();
            assert_eq!(us, frames * FRAME_US);
            seen += frames;
        }
        adder.join().expect("adder");
        let (frames, us) = clock.render.take();
        assert_eq!(us, frames * FRAME_US);
        assert_eq!(seen + frames, FRAMES, "no frame lost or counted twice");
    }

    #[test]
    fn a_span_under_way_across_a_hide_and_a_show_is_dropped() {
        let c = Clock::default();
        c.set_on(true);
        let old = c.stamp().expect("on");
        // Hidden and shown again before the span ends: it began in the earlier showing.
        c.set_on(false);
        c.set_on(true);
        let new = c.stamp().expect("on");
        assert_ne!(old.generation, new.generation);
        assert!(!c.render.add(old.generation, Duration::from_micros(40_000)), "the old showing's span is dropped");
        assert!(!c.main.add(old.generation, Duration::from_micros(40_000)));
        assert!(c.render.add(new.generation, Duration::from_micros(2_000)));
        assert_eq!(c.take(), Means { main: None, render: Some(2.0), wait: None });
        // Hidden alone (no new showing yet): a span that ends meanwhile is the showing's,
        // and the next showing starts empty.
        let s = c.stamp().expect("on");
        c.set_on(false);
        assert!(c.main.add(s.generation, Duration::from_micros(7_000)));
        c.set_on(true);
        assert_eq!(c.take(), Means::default());
    }

    #[test]
    fn the_fields_saturate_and_the_generation_wraps() {
        let a = Acc::default();
        a.start(255);
        assert!(a.add(255, Duration::from_secs(80_000)), "over 2^36 µs");
        assert!(a.add(255, Duration::from_secs(10)));
        assert_eq!(a.take(), (2, US_MAX), "the time saturates without carrying into the frames");
        a.0.store(pack(255, COUNT_MAX, 0), Ordering::Relaxed);
        a.add(255, Duration::from_micros(1));
        assert_eq!(unpack(a.0.load(Ordering::Relaxed)), (255, COUNT_MAX, 1), "the frames saturate without carrying into the generation");
        // The clock's generation wraps from 255 to 0 and still tells showings apart.
        let c = Clock::default();
        c.generation.store(255, Ordering::Relaxed);
        c.set_on(true);
        let g = c.stamp().expect("on").generation;
        assert_eq!(g, 0);
        assert!(!c.main.add(255, Duration::from_micros(1)));
        assert!(c.main.add(0, Duration::from_micros(1_000)));
        assert_eq!(c.take().main, Some(1.0));
    }

    #[test]
    fn the_gpu_mean_is_of_the_measurements_since_the_showing_began() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let m = [(at(0), 9.0), (at(100), 9.0), (at(600), 2.0), (at(700), 3.0), (at(800), f64::NAN)];
        let (mean, age) = mean_where(m, |t| t >= at(500), at(1_200)).expect("two since");
        assert!((mean - 2.5).abs() < 1e-9, "the earlier showing's 9 ms are left out: {mean}");
        assert!((age - 0.5).abs() < 1e-9, "{age}");
        assert_eq!(mean_where(m[..2].iter().copied(), |t| t >= at(500), at(1_200)), None, "none since: no GPU time yet");
    }

    #[test]
    fn the_recorder_follows_the_row_only_when_the_overlay_alone_added_it() {
        let overlay_alone = |measuring_first: bool, measuring_after: bool| {
            let mut app = App::new();
            if measuring_first {
                crate::graphics::ensure_render_diagnostics(&mut app);
            }
            let added = crate::graphics::add_render_diagnostics(&mut app);
            app.insert_resource(OverlayAddedDiagnostics(added));
            if measuring_after {
                crate::graphics::ensure_render_diagnostics(&mut app);
            }
            gates_recorder(&app)
        };
        assert!(overlay_alone(false, false));
        assert!(!overlay_alone(true, false), "a measuring path added them before the overlay");
        assert!(!overlay_alone(false, true), "a measuring path asked for them after the overlay");
        // Bevy's own (Tracy builds add the plugin with the renderer): not the overlay's.
        let mut tracy = App::new();
        tracy.add_plugins(RenderDiagnosticsPlugin);
        let added = crate::graphics::add_render_diagnostics(&mut tracy);
        tracy.insert_resource(OverlayAddedDiagnostics(added));
        assert!(!gates_recorder(&tracy));
    }
}
