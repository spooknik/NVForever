//! NPC call attempts, separate from actual character insertions/removals [design].

use std::fmt;

#[derive(Clone, Copy, Debug)]
pub enum NpcReason {
    EnteredLoaded,
    LeftUnloaded,
    Arrival,
    RetrySkip,
    Stuck,
    Door,
    LastStretch,
    NoPath,
    ScriptMove,
    HeldIdle,
    Wait,
    Carried,
    Other,
}

const NAMES: [&str; 13] = [
    "entered_loaded",
    "left_unloaded",
    "arrival",
    "retry_skip",
    "stuck",
    "door",
    "last_stretch",
    "no_path",
    "script_move",
    "held_idle",
    "wait",
    "carried",
    "other",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NpcCalls {
    /// Add calls, including failed ground probes and already-present actors.
    pub add: [u64; 13],
    /// Remove calls, including absent actors.
    pub remove: [u64; 13],
}

impl NpcCalls {
    fn record(&mut self, reason: NpcReason, remove: bool) {
        (if remove { &mut self.remove } else { &mut self.add })[reason as usize] += 1;
    }
}

#[derive(Default)]
pub struct NpcWork {
    pub total: NpcCalls,
    pub period: NpcCalls,
}

impl NpcWork {
    pub fn record(&mut self, reason: NpcReason, remove: bool) {
        self.total.record(reason, remove);
        self.period.record(reason, remove);
    }

    pub fn take_period(&mut self) -> NpcCalls {
        std::mem::take(&mut self.period)
    }
}

impl fmt::Display for NpcCalls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "npc_calls add/remove=[")?;
        for (i, name) in NAMES.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{name}:{}/{}", self.add[i], self.remove[i])?;
        }
        f.write_str("]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_and_attempt_directions_stay_separate_when_period_is_taken() {
        let mut w = NpcWork::default();
        w.record(NpcReason::EnteredLoaded, false);
        w.record(NpcReason::Arrival, true);
        w.record(NpcReason::Arrival, true);
        let p = w.take_period();
        assert_eq!(p, w.total);
        assert_eq!(w.period, NpcCalls::default());
        assert_eq!(p.add[NpcReason::EnteredLoaded as usize], 1);
        assert_eq!(p.remove[NpcReason::Arrival as usize], 2);
        assert!(p.to_string().contains("arrival:0/2"));
        w.record(NpcReason::RetrySkip, true);
        assert_eq!(w.total.remove.iter().sum::<u64>(), 3);
        assert_eq!(w.period.remove.iter().sum::<u64>(), 1);
    }
}
