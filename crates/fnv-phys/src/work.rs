//! Allocation-free accumulation of physics work, enabled explicitly by the caller.
//! Timings measure only the calls, not the diagnostic scans [design].

use crate::DirtyReasons;
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    pub count: u64,
    pub sum: f64,
    pub max: f64,
}

impl Sample {
    pub fn record(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        self.max = self.max.max(value);
    }

    pub fn mean(&self) -> f64 {
        if self.count == 0 { 0.0 } else { self.sum / self.count as f64 }
    }
}

/// A period or lifetime. Counts include zero-work steps/frames. All times are ms.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkStats {
    /// Awake dynamics, awake ragdoll dynamics, moving kinematics, active pairs, points.
    pub step: [Sample; 5],
    pub rapier: Sample,
    pub detect: Sample,
    pub detect_reasons: [Sample; 1024],
    /// Body adds/removes, collider adds/removes, sampled at the frame boundary.
    pub frame: [Sample; 4],
    pub characters: [u64; 2],
    #[cfg(feature = "profiler")]
    pub broad: Sample,
    #[cfg(feature = "profiler")]
    pub narrow: Sample,
}

impl Default for WorkStats {
    fn default() -> Self {
        Self {
            step: Default::default(),
            rapier: Sample::default(),
            detect: Sample::default(),
            detect_reasons: [Sample::default(); 1024],
            frame: Default::default(),
            characters: [0; 2],
            #[cfg(feature = "profiler")]
            broad: Sample::default(),
            #[cfg(feature = "profiler")]
            narrow: Sample::default(),
        }
    }
}

impl WorkStats {
    fn record_refresh(&mut self, reasons: DirtyReasons, ms: f64) {
        self.detect.record(ms);
        self.detect_reasons[reasons.bits() as usize].record(ms);
    }

    fn record_step(&mut self, counts: [u64; 5], ms: f64) {
        for (s, n) in self.step.iter_mut().zip(counts) {
            s.record(n as f64);
        }
        self.rapier.record(ms);
    }

    fn record_frame(&mut self, counts: [u64; 4]) {
        for (s, n) in self.frame.iter_mut().zip(counts) {
            s.record(n as f64);
        }
    }
}

/// Allocated once when enabled. No per-frame maps, vectors or clones.
#[derive(Clone, Debug, Default)]
pub(crate) struct WorkMeter {
    pub total: WorkStats,
    pub period: WorkStats,
    pub pending: [u64; 4],
}

impl WorkMeter {
    pub fn refresh(&mut self, reasons: DirtyReasons, ms: f64) {
        self.total.record_refresh(reasons, ms);
        self.period.record_refresh(reasons, ms);
    }

    pub fn step(&mut self, counts: [u64; 5], ms: f64) {
        self.total.record_step(counts, ms);
        self.period.record_step(counts, ms);
    }

    pub fn frame(&mut self) {
        let counts = std::mem::take(&mut self.pending);
        self.total.record_frame(counts);
        self.period.record_frame(counts);
    }

    pub fn character(&mut self, removed: bool) {
        self.total.characters[removed as usize] += 1;
        self.period.characters[removed as usize] += 1;
    }

    #[cfg(feature = "profiler")]
    pub fn phases(&mut self, broad: f64, narrow: f64) {
        self.total.broad.record(broad);
        self.period.broad.record(broad);
        self.total.narrow.record(narrow);
        self.period.narrow.record(narrow);
    }
}

impl fmt::Display for WorkStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "steps={}; per-step mean/max", self.rapier.count)?;
        for (name, s) in ["awake", "ragdoll", "moving_kinematic", "active_pairs", "manifold_points"]
            .into_iter()
            .zip(&self.step)
        {
            write!(f, " {name}={:.2}/{:.0}", s.mean(), s.max)?;
        }
        write!(
            f,
            "; rapier_ms total/mean/max={:.3}/{:.3}/{:.3}; detect_ms count/total/mean/max={}/{:.3}/{:.3}/{:.3}; detect_top3=[",
            self.rapier.sum,
            self.rapier.mean(),
            self.rapier.max,
            self.detect.count,
            self.detect.sum,
            self.detect.mean(),
            self.detect.max
        )?;
        let mut top = [None; 3];
        for (i, s) in self.detect_reasons.iter().enumerate().filter(|(_, s)| s.count > 0) {
            for rank in 0..3 {
                if top[rank].is_none_or(|j: usize| s.sum > self.detect_reasons[j].sum) {
                    for k in (rank + 1..3).rev() {
                        top[k] = top[k - 1];
                    }
                    top[rank] = Some(i);
                    break;
                }
            }
        }
        for (rank, i) in top.into_iter().flatten().enumerate() {
            let s = &self.detect_reasons[i];
            if rank > 0 {
                f.write_str(", ")?;
            }
            write!(
                f,
                "{}:{}/{:.3}/{:.3}/{:.3}",
                DirtyReasons::from_bits(i as u16),
                s.count,
                s.sum,
                s.mean(),
                s.max
            )?;
        }
        write!(f, "]; frames={}; per-frame total/mean/max", self.frame[0].count)?;
        for (name, s) in ["bodies_added", "bodies_removed", "colliders_added", "colliders_removed"]
            .into_iter()
            .zip(&self.frame)
        {
            write!(f, " {name}={:.0}/{:.2}/{:.0}", s.sum, s.mean(), s.max)?;
        }
        write!(f, "; characters_added={} removed={}", self.characters[0], self.characters[1])?;
        #[cfg(feature = "profiler")]
        write!(
            f,
            "; step_broad_ms total/mean/max={:.3}/{:.3}/{:.3}; step_narrow_ms total/mean/max={:.3}/{:.3}/{:.3}",
            self.broad.sum,
            self.broad.mean(),
            self.broad.max,
            self.narrow.sum,
            self.narrow.mean(),
            self.narrow.max
        )?;
        #[cfg(not(feature = "profiler"))]
        f.write_str("; profiler=off")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn means_include_zeroes_and_period_reset_preserves_totals_and_pending() {
        let mut m = WorkMeter::default();
        m.step([2, 1, 3, 4, 5], 2.0);
        m.step([0; 5], 0.0);
        m.pending = [4, 2, 8, 3];
        m.frame();
        m.frame();
        m.character(false);
        m.character(true);
        assert_eq!(
            m.total.step[0],
            Sample {
                count: 2,
                sum: 2.0,
                max: 2.0
            }
        );
        assert_eq!(m.total.step[0].mean(), 1.0);
        assert_eq!(m.total.frame[2].mean(), 4.0);
        assert_eq!(m.total.characters, [1, 1]);
        m.pending[0] = 7;
        let period = std::mem::take(&mut m.period);
        assert_eq!(period, m.total);
        assert_eq!(m.period, WorkStats::default());
        assert_eq!(m.pending[0], 7);
        assert_eq!(Sample::default().mean(), 0.0);
    }

    #[test]
    fn refresh_combinations_rank_by_total_time_with_stable_ties() {
        let mut m = WorkMeter::default();
        m.refresh(DirtyReasons::CHARACTERS, 3.0);
        m.refresh(DirtyReasons::BODIES | DirtyReasons::TERRAIN, 4.0);
        m.refresh(DirtyReasons::BODIES | DirtyReasons::TERRAIN, 2.0);
        m.refresh(DirtyReasons::PLACED, 3.0);
        m.refresh(DirtyReasons::DOORS, 1.0);
        assert_eq!(
            m.total.detect,
            Sample {
                count: 5,
                sum: 13.0,
                max: 4.0
            }
        );
        assert_eq!(m.total, m.period);
        assert!(
            m.total
                .to_string()
                .contains("detect_top3=[bodies+terrain:2/6.000/3.000/4.000, characters:1/3.000/3.000/3.000, placed:1/3.000/3.000/3.000]")
        );
        assert!(WorkStats::default().to_string().contains("detect_top3=[]"));
    }
}
