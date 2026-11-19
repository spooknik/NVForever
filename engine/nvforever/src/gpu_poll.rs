//! wgpu's clean-up of finished GPU work, moved off the render thread (#1427).
//!
//! [documented: wgpu-core 30.0.1 `device/queue.rs` `Queue::submit`, `device/life.rs`
//! `triage_submissions`] Each `Queue::submit` ends by polling the device: the submissions
//! the GPU has finished since the last poll are dropped there, with every resource
//! reference their command buffers held ("drop command buffer trackers"). Bevy polls the
//! device nowhere else, so that work runs on the render thread inside every frame's
//! `queue_submit`. [observed 2026-11-03, docs/design/frame-rate-audit.md §5] It was about
//! 0.8 ms of `queue_submit`'s 1.2 ms at Goodsprings.
//!
//! A thread of its own polls the device (without waiting) every millisecond, so a frame's
//! submissions are dropped soon after the GPU finishes them, while the render thread
//! records the next frame, and `queue_submit` finds them done. The work and the callbacks
//! (buffer mappings, work-done notices) are the same; they only run on this thread.
//! Nothing drawn changes.
//!
//! [`quiet`]: a caller that needs no Vulkan call from this thread for a while (DLSS Frame
//! Generation's live switch, `slSetFeatureLoaded`, "no other DXGI/D3D/Vulkan APIs should be
//! invoked in parallel", Streamline's `sl_core_api.h:130-131`) holds the gate's write side; each
//! poll holds its read side. Pipeline creation holds the same read side in vendored
//! `bevy_render::render_resource::pipeline_cache::create_pipeline_task`.
//!
//! [`stop`]: `main` ends the thread and waits for it once `App::run` has returned, before
//! `dlss::shutdown` and before `main` returns (#1825). Left running, it went on polling
//! while the process exited, and on Linux `exit` runs the libraries' destructors with every
//! other thread still going: NVIDIA's driver tore its state down under a poll in
//! `vkGetSemaphoreCounterValue` [observed: a player's core dump, RTX 4060, driver
//! 615.78.08, 2026-11-19: SIGSEGV at fault address 0xa0 in `libnvidia-glcore`, the `gpu poll`
//! thread in `wgpu_hal::vulkan` `get_fence_value`], so every run ended in SIGSEGV after
//! "the game exited normally". Windows ends the other threads before any DLL detaches,
//! which is why it never showed there [documented: Microsoft Learn, "Terminating a
//! Process": `ExitProcess` terminates the other threads, then sends `DLL_PROCESS_DETACH`].
//! The thread holds a clone of the device, dropped as it ends; Streamline keeps clones of its
//! own until after `slShutdown` (dlss/mod.rs "Shutdown"), so the device outlives that either way.

use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;
use bevy::render::{Render, RenderApp, RenderSystems};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLockWriteGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use bevy::render::render_resource::RENDER_DEVICE_API_GATE as GATE;

/// No poll or pipeline creation from task threads until the guard is dropped (the module comment).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn quiet() -> RwLockWriteGuard<'static, ()> {
    GATE.write().unwrap_or_else(PoisonError::into_inner)
}

/// How long the thread sleeps between polls.
const PERIOD: Duration = Duration::from_millis(1);

pub struct GpuPollPlugin;

impl Plugin for GpuPollPlugin {
    fn build(&self, app: &mut App) {
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(Render, start_poll.in_set(RenderSystems::Cleanup));
        }
    }
}

/// Starts the polling thread once the render device is there.
fn start_poll(device: Res<RenderDevice>, mut started: Local<bool>) {
    if std::mem::replace(&mut *started, true) {
        return;
    }
    let device = device.wgpu_device().clone();
    let spawned = Poller::spawn(move || {
        let _gate = GATE.read().unwrap_or_else(PoisonError::into_inner);
        device.poll(wgpu::PollType::Poll).map(drop).map_err(|e| e.to_string())
    });
    match spawned {
        Ok(p) => *POLLER.lock().unwrap_or_else(PoisonError::into_inner) = Some(p),
        Err(e) => warn!("gpu poll: no thread, {e}"),
    }
}

/// End the polling thread and wait for it (the module comment, [`stop`]). Does nothing when it
/// never started or has already been stopped.
pub fn stop() {
    let poller = POLLER.lock().unwrap_or_else(PoisonError::into_inner).take();
    if let Some(p) = poller {
        p.stop();
    }
}

/// The polling thread while it runs.
static POLLER: Mutex<Option<Poller>> = Mutex::new(None);

/// A thread calling `poll` every [`PERIOD`] until stopped or until `poll` fails.
struct Poller {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl Poller {
    fn spawn(mut poll: impl FnMut() -> Result<(), String> + Send + 'static) -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let thread = std::thread::Builder::new().name("gpu poll".into()).spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                std::thread::sleep(PERIOD);
                if stopping.load(Ordering::Acquire) {
                    break;
                }
                if let Err(e) = poll() {
                    warn!("gpu poll: stopped, {e}");
                    return;
                }
            }
        })?;
        Ok(Self { stop, thread })
    }

    /// Ask the thread to end and wait until it has: a poll under way finishes first, then
    /// `poll` (and the device it holds) is dropped on that thread.
    fn stop(self) {
        self.stop.store(true, Ordering::Release);
        if self.thread.join().is_err() {
            warn!("gpu poll: the thread panicked");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Poller;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    /// Dropped when the poller's closure is: stands in for the device it holds.
    struct Held(Arc<AtomicUsize>);
    impl Drop for Held {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn stop_ends_the_thread_and_drops_what_it_holds_before_returning() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicUsize::new(0));
        let (p, held) = (Arc::clone(&polls), Held(Arc::clone(&dropped)));
        let poller = Poller::spawn(move || {
            let _ = &held;
            p.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
        let start = Instant::now();
        while polls.load(Ordering::SeqCst) < 3 {
            assert!(start.elapsed() < Duration::from_secs(10), "the poller never polled");
            std::thread::yield_now();
        }
        poller.stop();
        assert_eq!(dropped.load(Ordering::SeqCst), 1, "the closure (and so the device) is dropped by the time stop returns");
        let after = polls.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(polls.load(Ordering::SeqCst), after, "no poll after stop");
    }

    #[test]
    fn a_failed_poll_ends_the_thread_and_stop_still_returns() {
        let poller = Poller::spawn(|| Err("device lost".into())).unwrap();
        poller.stop();
    }
}
