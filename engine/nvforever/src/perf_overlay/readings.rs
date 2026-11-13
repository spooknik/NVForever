//! The overlay's readings and the text of its rows: pure, so the numbers and their
//! wording are tested without a window (`super` gathers them, `super::draw` sets them).

use std::collections::VecDeque;

use bevy::math::UVec2;

use crate::memory_budget::{GPU_PRESSURE, RAM_PRESSURE};
use crate::upscaler::{DlssModel, DrawnUpscale, UpscalerChoice};
/// The loaded row's counts, as the stress samples read them.
pub use crate::stress_sample::Loaded;

/// How often the readout changes, in seconds: a number that changed every frame would
/// only flicker [design, as the counter of #1087].
pub const WINDOW: f32 = 0.5;

/// The seconds of frames the 1 % low is taken over [design]: over half a second (30
/// frames at 60 a second) the slowest hundredth is the slowest frame, so the low looks
/// back further than the rate and the mean; 10 s holds 600 frames at 60 a second, the
/// slowest 6 of them.
pub const LOW_SECS: f32 = 10.0;

/// The seconds of frames the frame graph draws [design: issue #1868, "the last few
/// seconds"].
pub const GRAPH_SECS: f32 = 5.0;

/// Which rows the overlay shows, each a switch of the settings page (`settings.rs`, kept
/// in `NVForever.ini`), and what the frame rate row's last figure is. The defaults are
/// issue #1868's. A later row is a field here, a row of [`rows`] and a switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rows {
    pub fps: bool,
    pub graph: bool,
    pub split: bool,
    pub cpu: bool,
    pub gpu: bool,
    pub vram: bool,
    pub ram: bool,
    pub loaded: bool,
    /// The upscaler that draws and its sizes (#2242, off by default).
    pub upscale: bool,
    /// The frame rate row's last figure (the settings' choice row).
    pub low: Low,
}

impl Default for Rows {
    fn default() -> Rows {
        Rows { fps: true, graph: false, split: false, cpu: true, gpu: true, vram: true, ram: true, loaded: false, upscale: false, low: Low::OnePercent }
    }
}

impl Rows {
    /// Whether a row switched on reads the frame times (else none are kept): the frame
    /// rate row and the graph.
    pub fn needs_frames(&self) -> bool {
        self.fps || self.graph
    }
}

/// The frame rate row's last figure: issue #1868's "1 % low (or the slowest frame, a
/// choice)", both over the last [`LOW_SECS`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Low {
    /// The mean of the slowest hundredth of the frames ([`low_of`]).
    #[default]
    OnePercent,
    /// The slowest frame.
    Slowest,
}

impl Low {
    pub const ALL: [Low; 2] = [Low::OnePercent, Low::Slowest];

    /// The settings page's name of the choice.
    pub fn name(self) -> &'static str {
        match self {
            Low::OnePercent => "1% low",
            Low::Slowest => "Slowest frame",
        }
    }

    /// As `NVForever.ini` keeps it.
    pub fn stored(self) -> &'static str {
        match self {
            Low::OnePercent => "low",
            Low::Slowest => "slowest",
        }
    }

    /// The choice of a page name or an INI value.
    pub fn parse(s: &str) -> Option<Low> {
        let s = s.trim();
        Low::ALL.into_iter().find(|l| l.name().eq_ignore_ascii_case(s) || l.stored().eq_ignore_ascii_case(s))
    }
}

/// The frame times of the readout's window (their count, sum and slowest, kept as they
/// come: no list to sort) and of the last [`LOW_SECS`] (a list, for the 1 % low),
/// milliseconds. Kept only while a row needs them (`super::Overlay::step`); frame
/// generation's count of generated frames is noted every frame
/// ([`Frames::note_generated`]).
#[derive(Default)]
pub struct Frames {
    count: u32,
    sum_ms: f32,
    worst_ms: f32,
    secs: f32,
    recent: VecDeque<f32>,
    recent_secs: f32,
    /// Reused for the 1 % low at each reading ([`low_of`]).
    scratch: Vec<f32>,
    /// Frame generation's count of generated frames shown (`frame_gen::GeneratedFrames`)
    /// as the last frame read it; kept when the frames are let go.
    generated: Option<u64>,
    /// The count where the window begins: as the frame before its first frame time read it.
    generated_from: Option<u64>,
}

impl Frames {
    /// A frame that took `dt` seconds.
    pub fn push(&mut self, dt: f32) {
        if dt <= 0.0 || !dt.is_finite() {
            return;
        }
        // A window's first frame time runs from the frame before, whose count is the one
        // last noted: its generated frames are counted from there.
        if self.generated_from.is_none() {
            self.generated_from = self.generated;
        }
        let ms = dt * 1e3;
        self.count += 1;
        self.sum_ms += ms;
        self.worst_ms = self.worst_ms.max(ms);
        self.secs += dt;
        self.recent.push_back(ms);
        self.recent_secs += dt;
        while self.recent_secs > LOW_SECS && self.recent.len() > 1 {
            let Some(old) = self.recent.pop_front() else { break };
            self.recent_secs -= old * 1e-3;
        }
    }

    /// Frame generation's count of generated frames shown as this frame read it (none
    /// without frame generation), noted every frame after its time is pushed, the overlay
    /// shown or not: a window that begins with the next frame time counts its generated
    /// frames from it, so the first reading of a showing, or of a row turned on, has all
    /// of its own [design].
    pub fn note_generated(&mut self, count: Option<u64>) {
        self.generated = count;
    }

    /// The reading of the frames since the last one (the window then starts again);
    /// none without a frame. Its generated frames are the count noted at its last frame
    /// less the count where it began; a generated frame of the window before that the
    /// presenter took back (`fg_presenter.rs`) can put the count below where this one
    /// began, which reads as none.
    pub fn take(&mut self) -> Option<FrameReading> {
        if self.count == 0 || self.secs <= 0.0 {
            return None;
        }
        let low_ms = low_of(self.recent.iter().copied(), &mut self.scratch).unwrap_or(self.worst_ms);
        let slowest_ms = self.recent.iter().copied().fold(self.worst_ms, f32::max);
        let generated = self.generated.zip(self.generated_from).map_or(0, |(now, from)| now.saturating_sub(from));
        let fps = self.count as f32 / self.secs;
        let shown_fps = (generated > 0).then(|| fps + generated as f32 / self.secs);
        let r = FrameReading { fps, mean_ms: self.sum_ms / self.count as f32, worst_ms: self.worst_ms, low_ms, slowest_ms, shown_fps };
        (self.count, self.sum_ms, self.worst_ms, self.secs) = (0, 0.0, 0.0, 0.0);
        // The next window's first frame time runs from this frame.
        self.generated_from = self.generated;
        Some(r)
    }

    /// The frame times of the last [`LOW_SECS`] (ms), the newest first (the graph's).
    pub fn newest_first(&self) -> impl Iterator<Item = f32> + '_ {
        self.recent.iter().rev().copied()
    }

    /// Whether no frame is kept.
    pub fn is_empty(&self) -> bool {
        self.count == 0 && self.recent.is_empty()
    }

    /// Forget every frame, and the room they took: a new showing reads afresh, and a row
    /// switched off keeps nothing. Frame generation's last noted count stays, for the next
    /// window to count from.
    pub fn clear(&mut self) {
        *self = Frames { generated: self.generated, ..Frames::default() };
    }
}

/// What the frame rate row reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameReading {
    pub fps: f32,
    pub mean_ms: f32,
    /// The window's slowest frame (the log's `slowest`, as the counter had it).
    pub worst_ms: f32,
    /// The mean of the slowest hundredth of the recent frames ([`low_of`]).
    pub low_ms: f32,
    /// The slowest of the recent frames (the frame rate row's other choice, [`Low`]).
    pub slowest_ms: f32,
    /// The frames shown a second while frame generation shows generated frames: the
    /// rendered ones (`fps`, whose times the figures above are) and the generated ones
    /// between them; none without a generated frame in the window.
    pub shown_fps: Option<f32>,
}

impl FrameReading {
    /// The frame rate row's last figure, as chosen.
    pub fn last_figure(&self, low: Low) -> f32 {
        match low {
            Low::OnePercent => self.low_ms,
            Low::Slowest => self.slowest_ms,
        }
    }
}

/// The 1 % low of frame times (ms): the mean of the slowest hundredth of them, one frame
/// at least, as `stress_sample::frame_stats` takes it; none without frames. The slowest
/// are picked out (`select_nth_unstable`), not sorted, in `scratch`, which keeps its room
/// from one reading to the next.
pub fn low_of(ms: impl Iterator<Item = f32>, scratch: &mut Vec<f32>) -> Option<f32> {
    scratch.clear();
    scratch.extend(ms);
    if scratch.is_empty() {
        return None;
    }
    let n = (scratch.len() / 100).max(1);
    if n < scratch.len() {
        scratch.select_nth_unstable_by(n - 1, |a, b| b.total_cmp(a));
    }
    Some(scratch[..n].iter().sum::<f32>() / n as f32)
}

/// Cores kept busy from the process's processor time (user and kernel, seconds) at two
/// moments `wall` seconds apart: 1.0 is one core's whole time [design: so a main thread
/// held at its limit reads at least 1.0 on any machine, where the machine's share would
/// read 6 % on 16 cores (issue #1868)]. None without time between them.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn cores_busy(cpu_before: f64, cpu_now: f64, wall: f64) -> Option<f32> {
    (wall > 0.0 && cpu_now >= cpu_before).then(|| ((cpu_now - cpu_before) / wall) as f32)
}

/// Every reading of a showing; `None` where it was not read (yet, or at all).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Readings {
    pub frame: Option<FrameReading>,
    /// The machine's processor use, percent.
    pub cpu: Option<f32>,
    /// The game's own processor use, cores ([`cores_busy`]).
    pub game_cores: Option<f32>,
    /// [design, #2249] GPU use in percent, with scope given by `gpu_game` (`usage.rs`).
    pub gpu: Option<f32>,
    /// [design, #2249] Whether this is game-only DRM client time (`usage::Shared`).
    pub gpu_game: bool,
    /// This process's GPU memory and its budget, MB (`MemoryBudget::vram_use_mb`).
    pub vram_mb: Option<(u64, u64)>,
    /// The game's working set, MB.
    pub game_ram_mb: Option<u64>,
    /// The machine's RAM in use, a share (`MemoryBudget::ram_share`).
    pub ram_share: Option<f64>,
    /// Whether the VRAM and the RAM row carry the warning mark ([`pressed_rows`]).
    pub vram_warn: bool,
    pub ram_warn: bool,
    /// The frame split row's times.
    pub split: Option<Split>,
    /// The loaded row's counts.
    pub loaded: Option<Loaded>,
    /// The upscaling row's facts, read every frame while the row is on (`super::sample`).
    pub upscale: Option<Upscale>,
}

/// What the upscaling row reads (#2242): the effective choice (`upscaler::effective_temporal`,
/// main world), the DLSS model asked for (`Graphics::dlss_model`; which preset NGX then uses
/// is not known to the engine), what the render world last drew the main camera with
/// (`upscaler::UpscaleDrawn`) and the window's size in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Upscale {
    pub effective: UpscalerChoice,
    pub model: DlssModel,
    pub drawn: Option<DrawnUpscale>,
    pub window: Option<UVec2>,
}

/// A frame's main-thread, render-thread and GPU time, ms, each the mean over the
/// readout's window (`super::split`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Split {
    pub main_ms: f32,
    /// The render schedule's whole span, the wait for the window's next image included;
    /// none until the render world has run a frame for the window.
    pub render_ms: Option<f32>,
    /// The wait for the window's next image, a part of `render_ms` (with vsync the
    /// display's time); none without `render_ms`.
    pub wait_ms: Option<f32>,
    /// None without GPU timestamps (the device does not offer them, or the render
    /// diagnostics were not added at the start).
    pub gpu_ms: Option<f32>,
}

/// One row: its label, its value and whether it warns. The graph row (`graph`) has no
/// text: the panel draws the frame graph there (`super::draw`).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: &'static str,
    pub value: String,
    pub warn: bool,
    pub graph: bool,
}

/// The rows' labels, in the order they are shown (the graph row's is empty); the label
/// column is as wide as the widest label shown.
pub const LABELS: [&str; 9] = ["FPS", "", "SPLIT", "CPU", "GPU", "VRAM", "RAM", "LOADED", "UPSCALING"];

/// The rows to show: those switched on, in [`LABELS`]' order, each left out when it has
/// nothing to read except CPU/GPU, which explain unavailable readings [design, #2249].
/// The frame rate row shows `--` until its first reading, so a showing starts with
/// something on screen, as the counter's `-- FPS` did; the graph row shows
/// from the start (its line grows as frames come).
pub fn rows(r: &Readings, on: &Rows) -> Vec<Row> {
    let fps = r.frame.as_ref().map_or_else(|| "--".to_string(), |f| fps_text(f, on.low));
    let parts = [
        (on.fps, Some(fps), false),
        (on.graph, Some(String::new()), false),
        (on.split, r.split.as_ref().map(split_text), false),
        (on.cpu, Some(cpu_text(r.cpu, r.game_cores)), false),
        (on.gpu, Some(gpu_text(r.gpu, r.gpu_game)), false),
        (on.vram, r.vram_mb.map(|(used, budget)| vram_text(used, budget)), r.vram_warn),
        (on.ram, ram_text(r.game_ram_mb, r.ram_share), r.ram_warn),
        (on.loaded, r.loaded.as_ref().map(loaded_text), false),
        (on.upscale, r.upscale.as_ref().map(upscale_text), false),
    ];
    LABELS
        .iter()
        .zip(parts)
        .enumerate()
        .filter_map(|(i, (&label, (shown, value, warn)))| Some(Row { label, value: value.filter(|_| shown)?, warn, graph: i == 1 }))
        .collect()
}

/// `60  16.7 ms  1% low 21.3` (or `slowest 21.3`, [`Low`]): the rate and the mean of the
/// window, the last 10 s's 1 % low or slowest frame. While frame generation shows
/// generated frames, the frames shown and the rendered ones apart,
/// `156  (78 rendered)  12.8 ms  1% low 14.0`, the times the rendered frames' [design:
/// the rendered frames' rate alone, which frame generation holds under half the refresh
/// rate, read as frame generation halving the frame rate]. The numbers are padded to
/// their usual width, so in the overlay's even-width font the row keeps its length while
/// they change.
pub fn fps_text(f: &FrameReading, low: Low) -> String {
    let name = match low {
        Low::OnePercent => "1% low",
        Low::Slowest => "slowest",
    };
    let rate = match f.shown_fps {
        Some(shown) => {
            let rendered = format!("({:.0}", f.fps);
            format!("{shown:>3.0} {rendered:>4} rendered)")
        }
        None => format!("{:>3.0}", f.fps),
    };
    format!("{rate}  {:>4.1} ms  {name} {:>4.1}", f.mean_ms, f.last_figure(low))
}

/// `main  5.5  render  6.3 (wait  4.0)  GPU  2.4 ms`: each part when read, padded as
/// [`fps_text`]; the render time includes the wait in brackets.
pub fn split_text(s: &Split) -> String {
    let mut out = format!("main {:>4.1}", s.main_ms);
    if let Some(r) = s.render_ms {
        out += &format!("  render {r:>4.1}");
        if let Some(w) = s.wait_ms {
            out += &format!(" (wait {w:>4.1})");
        }
    }
    if let Some(g) = s.gpu_ms {
        out += &format!("  GPU {g:>4.1}");
    }
    out + " ms"
}

/// ` 9 cells   2410 refs   12 actors   18240 entities`, padded as [`fps_text`].
pub fn loaded_text(l: &Loaded) -> String {
    format!("{:>2} cells  {:>5} refs  {:>3} actors  {:>6} entities", l.cells, l.refs, l.actors, l.entities)
}

/// `1280x720`.
fn size(s: UVec2) -> String {
    format!("{}x{}", s.x, s.y)
}

/// A choice as the upscaling row names it: `Off`, `Bilinear` and the mode in the language
/// shown, the other upscalers by their own names (`Upscaler::name`, brand names the language
/// files leave as they are): `FSR 3.1 Balanced`, `DLSS Quality`, `Bilinear Performance`.
fn choice_text(choice: UpscalerChoice) -> String {
    match choice.quality() {
        None => fnv_util::words::tr("Off").to_string(),
        Some(q) => format!("{} {}", fnv_util::words::tr(choice.upscaler().name()), fnv_util::words::tr(q.name())),
    }
}

/// The upscaling row (#2242), two spaces between its parts [design: the issue's examples]:
/// - drawn by the effective upscaler: the choice, render and output size, and for DLSS the
///   model asked for: `DLSS Quality  853x480 -> 1280x720  model Default`,
///   `FSR 3.1 Balanced  1129x635 -> 1920x1080`;
/// - the effective upscaler's camera drawn natively (an unusable view, no render size from
///   the backend): `Off  1280x720`, since native is what draws [design:
///   #2242's "what actually draws"];
/// - nothing effective: `Off` and the window's size;
/// - else (a switch the render world has not drawn yet, no camera, a loading screen): the
///   effective choice and `--`.
pub fn upscale_text(u: &Upscale) -> String {
    let off = || fnv_util::words::tr("Off").to_string();
    match (u.effective, u.drawn) {
        (e, Some(DrawnUpscale::Upscaled { choice, render, output })) if choice == e => {
            let mut text = format!("{}  {} -> {}", choice_text(choice), size(render), size(output));
            if choice.is_dlss() {
                text += "  ";
                text += &fnv_util::words::trf("model {model}", &[("model", &fnv_util::words::tr(u.model.name()))]);
            }
            text
        }
        (e, Some(DrawnUpscale::Native { choice, output })) if choice == e => format!("{}  {}", off(), size(output)),
        (UpscalerChoice::Off, _) => match u.window {
            Some(window) => format!("{}  {}", off(), size(window)),
            None => off(),
        },
        (e, _) => format!("{}  --", choice_text(e)),
    }
}

/// How old the last GPU time of a render pass may be for the frame split to count it,
/// seconds: a pass that stopped running keeps its old figure in the diagnostics store
/// [design].
pub const GPU_STALE_SECS: f64 = 1.0;

/// The GPU time of a frame (ms) from Bevy's render diagnostics `(path, mean of its kept
/// measurements, seconds since its last)`: the sum over the top-level passes
/// (`render/<pass>/elapsed_gpu`; a nested pass is inside its parent's time) measured in
/// the last [`GPU_STALE_SECS`], as `graphics.rs`'s `gpu time:` line sums them. None
/// without any (no timestamps).
pub fn gpu_frame_ms<'a>(passes: impl IntoIterator<Item = (&'a str, f64, f64)>) -> Option<f32> {
    let mut total = None;
    for (path, ms, age) in passes {
        let Some(pass) = path.strip_prefix("render/").and_then(|p| p.strip_suffix("/elapsed_gpu")) else { continue };
        if pass.contains('/') || !(0.0..=GPU_STALE_SECS).contains(&age) || !ms.is_finite() {
            continue;
        }
        *total.get_or_insert(0.0) += ms;
    }
    total.map(|t| t as f32)
}

/// ` 34%`, padded to three digits.
pub fn percent(p: f32) -> String {
    format!("{p:>3.0}%")
}

/// [design, #2249] Missing machine use remains explicit even when process time is read.
pub fn cpu_text(machine: Option<f32>, game_cores: Option<f32>) -> String {
    let mut text = machine.map_or_else(unavailable, percent);
    if let Some(c) = game_cores {
        text += &format!("  game {c:.1} cores");
    }
    text
}

/// [design, #2249] Own wording for unsupported host telemetry, translated like settings.
fn unavailable() -> String {
    fnv_util::words::tr("Not available").to_string()
}

/// [design, #2249] DRM time is the game's contribution; other GPU sources are device-wide.
pub fn gpu_text(gpu: Option<f32>, game: bool) -> String {
    match gpu {
        None => unavailable(),
        Some(p) if game => fnv_util::words::trf("game {percent}%", &[("percent", &format!("{p:>3.0}"))]),
        Some(p) => percent(p),
    }
}

/// MB as GB (1024 MB each, as Windows counts them) to a tenth: `4.7`.
pub fn gb(mb: u64) -> String {
    format!("{:.1}", mb as f64 / 1024.0)
}

/// `2.1 / 11.4 GB`: what the process holds of the GPU budget.
pub fn vram_text(used_mb: u64, budget_mb: u64) -> String {
    format!("{} / {} GB", gb(used_mb), gb(budget_mb))
}

/// `4.7 GB (machine 62%)`: the game's working set and the machine's RAM in use, each
/// when read.
pub fn ram_text(game_mb: Option<u64>, machine_share: Option<f64>) -> Option<String> {
    let machine = machine_share.map(|s| format!("machine {:.0}%", (s * 100.0).clamp(0.0, 100.0)));
    match (game_mb.map(|mb| format!("{} GB", gb(mb))), machine) {
        (Some(g), Some(m)) => Some(format!("{g} ({m})")),
        (g, m) => g.or(m),
    }
}

/// Which of the VRAM and the RAM row carry the warning mark: while the asset caches are
/// pressed (`MemoryBudget::pressed`), the row whose share keeps them pressed, at or over
/// its let-go share (`memory_budget::GPU_PRESSURE` / `RAM_PRESSURE`); none while they are
/// not [design: the budget only says that one of the two pressed them].
pub fn pressed_rows(pressed: bool, vram_share: Option<f64>, ram_share: Option<f64>) -> (bool, bool) {
    if !pressed {
        return (false, false);
    }
    (vram_share.is_some_and(|s| s >= GPU_PRESSURE.1), ram_share.is_some_and(|s| s >= RAM_PRESSURE.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_reading_without_a_frame() {
        let mut f = Frames::default();
        assert_eq!(f.take(), None);
        // Frames that took no time (a paused clock) or none at all are not frames.
        f.push(0.0);
        f.push(f32::NAN);
        assert_eq!(f.take(), None);
        assert!(f.is_empty());
    }

    #[test]
    fn a_reading_gives_rate_mean_slowest_and_the_low_of_the_recent_frames() {
        let mut f = Frames::default();
        // 10 s before the window: 150 frames of 10 ms and two of 50 ms.
        for _ in 0..150 {
            f.push(0.01);
        }
        f.push(0.05);
        f.push(0.05);
        f.take();
        // The window: 48 frames of 10 ms and one of 20 ms. The recent frames are 201
        // (198 of 10 ms, one of 20 ms, two of 50 ms); their slowest 1 % is the two 50s.
        for _ in 0..48 {
            f.push(0.01);
        }
        f.push(0.02);
        let r = f.take().expect("frames");
        assert!((r.fps - 49.0 / 0.5).abs() < 0.01, "{r:?}");
        assert!((r.mean_ms - 500.0 / 49.0).abs() < 0.01, "{r:?}");
        assert!((r.worst_ms - 20.0).abs() < 0.01, "{r:?}");
        assert!((r.low_ms - 50.0).abs() < 0.01, "{r:?}");
        assert_eq!(f.take(), None, "the window starts again");
    }

    #[test]
    fn the_low_is_the_stress_samples_low_without_sorting() {
        let mut scratch = Vec::new();
        assert_eq!(low_of(std::iter::empty(), &mut scratch), None);
        let mut ms: Vec<f32> = (0..1234).map(|i| ((i * 7919) % 1000) as f32 / 10.0).collect();
        ms.extend([250.0, 120.0, 99.5]);
        for take in [1, 50, 99, 100, 101, 250, ms.len()] {
            let part = &ms[..take];
            let low = low_of(part.iter().copied(), &mut scratch).expect("frames");
            let sorted = crate::stress_sample::frame_stats(part).1;
            assert!((low - sorted).abs() < 1e-3, "{take} frames: {low} against {sorted}");
        }
        // The scratch keeps its room for the next reading.
        assert!(scratch.capacity() >= ms.len());
    }

    #[test]
    fn frames_keep_the_last_ten_seconds_for_the_low() {
        let mut f = Frames::default();
        // A 200 ms hitch, then 11 s of 10 ms frames: the hitch falls out of the low.
        f.push(0.2);
        for _ in 0..31 {
            f.push(0.01);
        }
        let first = f.take().expect("0.51 s");
        assert!((first.low_ms - 200.0).abs() < 0.01, "{first:?}");
        for _ in 0..1100 {
            f.push(0.01);
        }
        let later = f.take().expect("a full window");
        assert!((later.low_ms - 10.0).abs() < 0.01, "{later:?}");
        assert!((later.worst_ms - 10.0).abs() < 0.01, "{later:?}");
        f.clear();
        assert!(f.is_empty());
        assert!(f.take().is_none());
    }

    #[test]
    fn cores_are_processor_time_over_time_passed() {
        assert_eq!(cores_busy(10.0, 11.5, 1.0), Some(1.5));
        assert_eq!(cores_busy(10.0, 10.5, 2.0), Some(0.25));
        assert_eq!(cores_busy(10.0, 11.0, 0.0), None, "no time passed");
        assert_eq!(cores_busy(10.0, 9.0, 1.0), None, "a clock that went back");
    }

    #[test]
    fn the_frame_rate_row_reads_like_the_counter() {
        let f = FrameReading { fps: 59.6, mean_ms: 16.78, worst_ms: 21.0, low_ms: 24.04, slowest_ms: 40.0, shown_fps: None };
        assert_eq!(fps_text(&f, Low::OnePercent), " 60  16.8 ms  1% low 24.0");
        let fast = FrameReading { fps: 144.0, mean_ms: 6.94, worst_ms: 9.0, low_ms: 8.5, slowest_ms: 9.5, shown_fps: None };
        assert_eq!(fps_text(&fast, Low::OnePercent), "144   6.9 ms  1% low  8.5");
        // Frame generation: the frames shown, then the rendered ones, as long whatever
        // their digits (a 240 Hz screen's cap is 117).
        let fg = FrameReading { fps: 77.9, mean_ms: 12.84, worst_ms: 14.0, low_ms: 14.04, slowest_ms: 15.0, shown_fps: Some(155.8) };
        assert_eq!(fps_text(&fg, Low::OnePercent), "156  (78 rendered)  12.8 ms  1% low 14.0");
        let fg_240 = FrameReading { fps: 117.0, mean_ms: 8.54, shown_fps: Some(234.0), ..fg };
        assert_eq!(fps_text(&fg_240, Low::Slowest), "234 (117 rendered)   8.5 ms  slowest 15.0");
        assert_eq!(fps_text(&fg, Low::OnePercent).len(), fps_text(&fg_240, Low::OnePercent).len());
    }

    /// Half a second at frame generation's cap on a 160 Hz screen: 39 frames of 1/78 s.
    const CAP_DT: f32 = 1.0 / 78.0;
    const CAP_WINDOW: usize = 39;

    /// A frame as `super::sample` takes it: its time, then frame generation's count as it
    /// read it.
    fn frame(f: &mut Frames, dt: f32, generated: Option<u64>) {
        f.push(dt);
        f.note_generated(generated);
    }

    /// `n` frames at the cap, each with a generated frame shown after it; the count after.
    fn generating(f: &mut Frames, mut count: u64, n: usize) -> u64 {
        for _ in 0..n {
            count += 1;
            frame(f, CAP_DT, Some(count));
        }
        count
    }

    fn shown(r: &FrameReading) -> f32 {
        r.shown_fps.expect("frames shown")
    }

    #[test]
    fn frame_generation_counts_the_generated_frames_of_each_window() {
        let mut f = Frames::default();
        f.note_generated(Some(1000));
        // A generated frame after each rendered one: twice the rendered rate.
        let count = generating(&mut f, 1000, CAP_WINDOW);
        let r = f.take().expect("frames");
        assert!((r.fps - 78.0).abs() < 0.01, "{r:?}");
        assert!((shown(&r) - 156.0).abs() < 0.01, "{r:?}");
        assert_eq!(fps_text(&r, Low::OnePercent), "156  (78 rendered)  12.8 ms  1% low 12.8");
        // The next window counts from where this one ended: the same, not a frame more or
        // fewer.
        generating(&mut f, count, CAP_WINDOW);
        let r = f.take().expect("frames");
        assert!((shown(&r) - 156.0).abs() < 0.01, "{r:?}");
    }

    #[test]
    fn frame_generation_on_for_part_of_a_window_counts_what_it_generated() {
        let mut f = Frames::default();
        f.note_generated(Some(0));
        // 20 frames with nothing generated (a menu), then 19 with: 38 generated a second
        // over the window beside the 78 rendered.
        for _ in 0..20 {
            frame(&mut f, CAP_DT, Some(0));
        }
        let count = generating(&mut f, 0, 19);
        let r = f.take().expect("frames");
        assert!((shown(&r) - 116.0).abs() < 0.01, "{r:?}");
        // Off for the next window: the rendered frames' reading alone.
        for _ in 0..CAP_WINDOW {
            frame(&mut f, CAP_DT, Some(count));
        }
        let r = f.take().expect("frames");
        assert_eq!(r.shown_fps, None);
        assert_eq!(fps_text(&r, Low::OnePercent), " 78  12.8 ms  1% low 12.8");
    }

    #[test]
    fn a_generated_frame_taken_back_counts_against_the_window_it_lands_in() {
        let mut f = Frames::default();
        f.note_generated(Some(0));
        let count = generating(&mut f, 0, CAP_WINDOW);
        assert!(f.take().is_some_and(|r| (shown(&r) - 156.0).abs() < 0.01));
        // The presenter leaves out the last window's last generated frame to catch up:
        // taken back at this window's first frame, which shows one generated frame fewer.
        frame(&mut f, CAP_DT, Some(count));
        let count = generating(&mut f, count, CAP_WINDOW - 1);
        let r = f.take().expect("frames");
        assert!((shown(&r) - 154.0).abs() < 0.01, "{r:?}");
        // Frame generation switched off and a take-back landing after the window began:
        // the count below where it began reads as nothing generated, not as a count
        // wrapped round.
        for _ in 0..CAP_WINDOW {
            frame(&mut f, CAP_DT, Some(count - 1));
        }
        let r = f.take().expect("frames");
        assert_eq!(r.shown_fps, None, "{r:?}");
        assert!((r.fps - 78.0).abs() < 0.01, "{r:?}");
    }

    #[test]
    fn a_new_showing_counts_from_the_frame_before_its_first_frame_time() {
        let mut f = Frames::default();
        // Hidden while frame generation runs: no frame kept, the count noted each frame.
        let mut count = 0;
        for _ in 0..100 {
            count += 1;
            f.note_generated(Some(count));
        }
        // Shown (`super::switch` lets the frames go): the first frame time runs from the
        // last hidden frame, so the first reading has all 39 generated frames, not 38.
        f.clear();
        count = generating(&mut f, count, CAP_WINDOW);
        let r = f.take().expect("frames");
        assert!((shown(&r) - 156.0).abs() < 0.01, "{r:?}");
        // Hidden and shown again, or the frame rows off and on again (the frames let go
        // the same way): the same.
        f.clear();
        for _ in 0..50 {
            count += 1;
            f.note_generated(Some(count));
        }
        f.clear();
        generating(&mut f, count, CAP_WINDOW);
        let r = f.take().expect("frames");
        assert!((shown(&r) - 156.0).abs() < 0.01, "{r:?}");
    }

    #[test]
    fn without_frame_generation_the_row_reads_the_rendered_frames() {
        let mut f = Frames::default();
        for _ in 0..30 {
            frame(&mut f, 1.0 / 60.0, None);
        }
        let r = f.take().expect("frames");
        assert_eq!(r.shown_fps, None);
        assert_eq!(fps_text(&r, Low::OnePercent), " 60  16.7 ms  1% low 16.7");
    }

    #[test]
    fn the_slowest_frame_choice_reads_the_slowest_of_the_last_ten_seconds() {
        let mut f = Frames::default();
        // A 120 ms hitch, then 2 s of 10 ms frames over four readings: the window's
        // slowest is 10 ms, the last 10 s's slowest the hitch, its 1 % low the hitch and
        // the slowest 10 ms frame (200 frames and the hitch: the two slowest).
        f.push(0.12);
        for _ in 0..4 {
            for _ in 0..50 {
                f.push(0.01);
            }
            f.take();
        }
        for _ in 0..50 {
            f.push(0.01);
        }
        let r = f.take().expect("frames");
        assert!((r.worst_ms - 10.0).abs() < 0.01, "{r:?}");
        assert!((r.slowest_ms - 120.0).abs() < 0.01, "{r:?}");
        assert!((r.low_ms - 65.0).abs() < 0.01, "{r:?}");
        assert_eq!(fps_text(&r, Low::Slowest), "100  10.0 ms  slowest 120.0");
        assert_eq!(fps_text(&r, Low::OnePercent), "100  10.0 ms  1% low 65.0");
        // The choice is kept by its INI name and read back by either name.
        for l in Low::ALL {
            assert_eq!(Low::parse(l.stored()), Some(l));
            assert_eq!(Low::parse(l.name()), Some(l));
        }
        assert_eq!(Low::parse("slowest"), Some(Low::Slowest));
        assert_eq!(Low::parse("fastest"), None);
        assert_eq!(Rows::default().low, Low::OnePercent);
    }

    #[test]
    fn the_split_row_shows_each_time_it_has() {
        let all = Split { main_ms: 5.54, render_ms: Some(6.32), wait_ms: Some(4.04), gpu_ms: Some(2.38) };
        assert_eq!(split_text(&all), "main  5.5  render  6.3 (wait  4.0)  GPU  2.4 ms");
        let no_gpu = Split { gpu_ms: None, ..all };
        assert_eq!(split_text(&no_gpu), "main  5.5  render  6.3 (wait  4.0) ms");
        let no_wait = Split { wait_ms: None, ..all };
        assert_eq!(split_text(&no_wait), "main  5.5  render  6.3  GPU  2.4 ms");
        let long_wait = Split { main_ms: 3.1, render_ms: Some(16.6), wait_ms: Some(12.24), gpu_ms: Some(2.0) };
        assert_eq!(split_text(&long_wait), "main  3.1  render 16.6 (wait 12.2)  GPU  2.0 ms", "with vsync the wait is most of render");
        let slow = Split { main_ms: 31.0, render_ms: None, wait_ms: None, gpu_ms: Some(12.0) };
        assert_eq!(split_text(&slow), "main 31.0  GPU 12.0 ms");
    }

    #[test]
    fn the_gpu_time_is_the_top_level_passes_measured_lately() {
        let passes = [
            ("render/main_opaque_pass_3d/elapsed_gpu", 3.0, 0.1),
            ("render/main_opaque_pass_3d/sub/elapsed_gpu", 1.0, 0.1),
            ("render/tonemapping/elapsed_gpu", 0.5, 0.0),
            ("render/tonemapping/elapsed_cpu", 9.0, 0.0),
            ("render/old_pass/elapsed_gpu", 7.0, 5.0),
            ("else/elapsed_gpu", 2.0, 0.0),
        ];
        assert_eq!(gpu_frame_ms(passes), Some(3.5));
        assert_eq!(gpu_frame_ms([("render/tonemapping/elapsed_cpu", 1.0, 0.0)]), None, "no timestamps: no GPU time");
        assert_eq!(gpu_frame_ms([]), None);
    }

    #[test]
    fn the_loaded_row_counts_what_is_loaded() {
        let l = Loaded { cells: 9, refs: 2410, actors: 12, entities: 18_240 };
        assert_eq!(loaded_text(&l), " 9 cells   2410 refs   12 actors   18240 entities");
        let empty = Loaded::default();
        assert_eq!(loaded_text(&empty), " 0 cells      0 refs    0 actors       0 entities");
    }

    #[test]
    fn the_cpu_row_shows_what_was_read() {
        assert_eq!(cpu_text(None, None), "Not available");
        assert_eq!(cpu_text(Some(34.4), Some(1.62)), " 34%  game 1.6 cores");
        assert_eq!(cpu_text(Some(5.0), None), "  5%");
        assert_eq!(cpu_text(None, Some(0.04)), "Not available  game 0.0 cores");
        assert_eq!(percent(100.0), "100%");
    }

    #[test]
    fn memory_is_shown_in_gb_to_a_tenth() {
        assert_eq!(gb(0), "0.0");
        assert_eq!(gb(51), "0.0");
        assert_eq!(gb(52), "0.1");
        assert_eq!(gb(1024), "1.0");
        assert_eq!(gb(4813), "4.7");
        assert_eq!(gb(11_673), "11.4");
        assert_eq!(vram_text(2150, 11_673), "2.1 / 11.4 GB");
        assert_eq!(ram_text(Some(4813), Some(0.6249)).as_deref(), Some("4.7 GB (machine 62%)"));
        assert_eq!(ram_text(Some(4813), None).as_deref(), Some("4.7 GB"));
        assert_eq!(ram_text(None, Some(0.5)).as_deref(), Some("machine 50%"));
        assert_eq!(ram_text(None, None), None);
    }

    #[test]
    fn rows_are_those_switched_on_with_something_to_read() {
        let on = Rows::default();
        // [design, #2249] A showing starts with waiting FPS and explicit unavailable usage.
        let waiting = rows(&Readings::default(), &on);
        assert_eq!(
            waiting,
            vec![
                Row { label: "FPS", value: "--".into(), warn: false, graph: false },
                Row { label: "CPU", value: "Not available".into(), warn: false, graph: false },
                Row { label: "GPU", value: "Not available".into(), warn: false, graph: false }
            ]
        );
        let r = Readings {
            frame: Some(FrameReading { fps: 60.0, mean_ms: 16.7, worst_ms: 20.0, low_ms: 21.3, slowest_ms: 30.0, shown_fps: None }),
            cpu: Some(12.0),
            game_cores: Some(1.6),
            gpu: Some(71.0),
            gpu_game: false,
            vram_mb: Some((2150, 11_673)),
            game_ram_mb: Some(4813),
            ram_share: Some(0.62),
            vram_warn: true,
            ram_warn: false,
            split: Some(Split { main_ms: 9.8, render_ms: Some(4.1), wait_ms: Some(1.5), gpu_ms: None }),
            loaded: Some(Loaded { cells: 9, refs: 2410, actors: 12, entities: 18_240 }),
            upscale: Some(Upscale { effective: UpscalerChoice::Off, model: DlssModel::Default, drawn: None, window: Some(UVec2::new(1280, 720)) }),
        };
        let defaults = rows(&r, &on);
        assert_eq!(defaults.iter().map(|r| r.label).collect::<Vec<_>>(), ["FPS", "CPU", "GPU", "VRAM", "RAM"]);
        assert_eq!(defaults[0].value, " 60  16.7 ms  1% low 21.3");
        assert_eq!(defaults[2].value, " 71%");
        let game = rows(&Readings { gpu_game: true, ..r.clone() }, &on);
        assert_eq!(game[2].value, "game  71%");
        assert_eq!(gpu_text(None, true), "Not available");
        assert!(defaults[3].warn && !defaults[4].warn);
        // Every row: the graph under the frame rate, the split under it, the counts, then the
        // upscaling row last.
        let every = Rows { graph: true, split: true, loaded: true, upscale: true, ..on };
        let all = rows(&r, &every);
        assert_eq!(all.iter().map(|r| r.label).collect::<Vec<_>>(), LABELS);
        assert_eq!(all.iter().map(|r| r.graph).collect::<Vec<_>>(), [false, true, false, false, false, false, false, false, false]);
        assert_eq!(all[2].value, "main  9.8  render  4.1 (wait  1.5) ms");
        assert_eq!(all[7].value, " 9 cells   2410 refs   12 actors   18240 entities");
        assert_eq!(all[8].value, "Off  1280x720");
        // The upscaling row is off by default.
        assert!(!Rows::default().upscale);
        assert!(!defaults.iter().any(|r| r.label == "UPSCALING"));
        // The slowest frame chosen.
        assert_eq!(rows(&r, &Rows { low: Low::Slowest, ..on })[0].value, " 60  16.7 ms  slowest 30.0");
        // Only the frame rate row: one line, as the counter was.
        let none = Rows { fps: false, graph: false, split: false, cpu: false, gpu: false, vram: false, ram: false, loaded: false, upscale: false, low: Low::OnePercent };
        let fps_only = Rows { fps: true, ..none };
        assert_eq!(rows(&r, &fps_only).len(), 1);
        // [design, #2249] Missing usage stays explicit; budget, split and counts await data.
        let bare = Readings { frame: r.frame, ..Readings::default() };
        assert_eq!(rows(&bare, &every).iter().map(|r| r.label).collect::<Vec<_>>(), ["FPS", "", "CPU", "GPU"]);
        // The upscaling row before its first reading: left out like the counts.
        assert!(!rows(&bare, &every).iter().any(|r| r.label == "UPSCALING"));
        // Every row off: nothing to draw.
        assert!(rows(&r, &none).is_empty());
    }

    #[test]
    fn the_upscaling_row_says_what_draws_and_at_what_size() {
        use crate::upscaler::UpscaleQuality::*;
        let (render, output) = (UVec2::new(853, 480), UVec2::new(1280, 720));
        let window = Some(output);
        let reading = |effective, drawn| Upscale { effective, model: DlssModel::Default, drawn, window };
        let upscaled = |choice, render| Some(DrawnUpscale::Upscaled { choice, render, output });
        // Drawn by the effective upscaler: the sizes, and for DLSS the model.
        let dlss = UpscalerChoice::Dlss(Quality);
        assert_eq!(upscale_text(&reading(dlss, upscaled(dlss, render))), "DLSS Quality  853x480 -> 1280x720  model Default");
        for model in DlssModel::ALL {
            let text = upscale_text(&Upscale { model, ..reading(dlss, upscaled(dlss, render)) });
            assert_eq!(text, format!("DLSS Quality  853x480 -> 1280x720  model {}", model.name()), "{model:?}");
        }
        let fsr = UpscalerChoice::Fsr(Balanced);
        let fhd = Upscale { effective: fsr, model: DlssModel::K, drawn: Some(DrawnUpscale::Upscaled { choice: fsr, render: UVec2::new(1129, 635), output: UVec2::new(1920, 1080) }), window: Some(UVec2::new(1920, 1080)) };
        assert_eq!(upscale_text(&fhd), "FSR 3.1 Balanced  1129x635 -> 1920x1080", "no model for FSR");
        let native_aa = UpscalerChoice::Fsr(NativeAa);
        assert_eq!(upscale_text(&reading(native_aa, upscaled(native_aa, output))), "FSR 3.1 Native AA  1280x720 -> 1280x720");
        let dlaa = UpscalerChoice::Dlss(NativeAa);
        assert_eq!(upscale_text(&reading(dlaa, upscaled(dlaa, output))), "DLSS Native AA  1280x720 -> 1280x720  model Default");
        // The effective upscaler's camera drawn natively: native is what draws.
        assert_eq!(upscale_text(&reading(dlss, Some(DrawnUpscale::Native { choice: dlss, output }))), "Off  1280x720");
        // Nothing effective: Off and the window, whatever the render world drew last.
        assert_eq!(upscale_text(&reading(UpscalerChoice::Off, None)), "Off  1280x720");
        assert_eq!(upscale_text(&reading(UpscalerChoice::Off, upscaled(dlss, render))), "Off  1280x720", "switched off, not drawn yet");
        assert_eq!(upscale_text(&Upscale { window: None, ..reading(UpscalerChoice::Off, None) }), "Off", "no window to measure");
        // A switch not drawn yet, no camera: the effective choice and no sizes.
        let performance = UpscalerChoice::Fsr(Performance);
        assert_eq!(upscale_text(&reading(performance, upscaled(UpscalerChoice::Fsr(Quality), render))), "FSR 3.1 Performance  --", "another mode drawn");
        assert_eq!(upscale_text(&reading(performance, upscaled(dlss, render))), "FSR 3.1 Performance  --", "another upscaler drawn");
        assert_eq!(upscale_text(&reading(dlss, None)), "DLSS Quality  --", "no camera drawn");
        assert_eq!(upscale_text(&reading(dlss, Some(DrawnUpscale::Native { choice: UpscalerChoice::Fsr(Quality), output }))), "DLSS Quality  --");
        // Bilinear (#2414): FSR's sizes, no model.
        let bilinear = UpscalerChoice::Bilinear(Performance);
        assert_eq!(upscale_text(&reading(bilinear, upscaled(bilinear, UVec2::new(640, 360)))), "Bilinear Performance  640x360 -> 1280x720");
        assert_eq!(upscale_text(&reading(bilinear, None)), "Bilinear Performance  --", "not drawn yet");
    }

    #[test]
    fn the_mark_goes_on_the_row_that_keeps_the_caches_pressed() {
        assert_eq!(pressed_rows(false, Some(0.95), Some(0.95)), (false, false), "not pressed: no mark");
        assert_eq!(pressed_rows(true, Some(0.9), Some(0.5)), (true, false));
        assert_eq!(pressed_rows(true, Some(0.5), Some(0.92)), (false, true));
        assert_eq!(pressed_rows(true, Some(0.8), None), (true, false), "between press and let go");
        assert_eq!(pressed_rows(true, None, None), (false, false));
    }
}
