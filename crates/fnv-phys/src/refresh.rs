//! Why scene queries had to be brought up to date between steps
//! ([`crate::PhysWorld::update_queries`]): every change that leaves the broad phase stale
//! says what it was ([`DirtyReasons`]), the reasons accumulate until the next refresh, and
//! each refresh is counted by the combination of reasons that called for it
//! ([`RefreshStats`]). Measurement only: the reasons do not change what a refresh does.

use std::collections::BTreeMap;
use std::fmt;

/// A set of reasons the broad phase is stale (a bit set; [`DirtyReasons::NONE`] when it is
/// up to date).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DirtyReasons(u16);

impl DirtyReasons {
    pub const NONE: DirtyReasons = DirtyReasons(0);
    /// An actor's body part capsules were set ([`crate::PhysWorld::set_actor_parts`]).
    pub const ACTOR_PARTS: DirtyReasons = DirtyReasons(1 << 0);
    /// An actor's single hit capsule was set ([`crate::PhysWorld::set_actor_volume`]).
    pub const ACTOR_VOLUME: DirtyReasons = DirtyReasons(1 << 1);
    /// Bodies were added or removed (references, actor volumes, characters).
    pub const BODIES: DirtyReasons = DirtyReasons(1 << 2);
    /// A keyframed body (a door) was moved or made solid or not.
    pub const DOORS: DirtyReasons = DirtyReasons(1 << 3);
    /// A character was added.
    pub const CHARACTERS: DirtyReasons = DirtyReasons(1 << 4);
    /// A body was placed by setting its position (a teleport).
    pub const PLACED: DirtyReasons = DirtyReasons(1 << 5);
    /// A ragdoll was built or cut.
    pub const RAGDOLLS: DirtyReasons = DirtyReasons(1 << 6);
    /// A gore piece was added.
    pub const GORE: DirtyReasons = DirtyReasons(1 << 7);
    /// Terrain was added or removed.
    pub const TERRAIN: DirtyReasons = DirtyReasons(1 << 8);
    /// Bodies added asleep need their first contacts before a step.
    pub const SLEEP: DirtyReasons = DirtyReasons(1 << 9);

    const NAMES: [(DirtyReasons, &'static str); 10] = [
        (DirtyReasons::ACTOR_PARTS, "actor_parts"),
        (DirtyReasons::ACTOR_VOLUME, "actor_volume"),
        (DirtyReasons::BODIES, "bodies"),
        (DirtyReasons::DOORS, "doors"),
        (DirtyReasons::CHARACTERS, "characters"),
        (DirtyReasons::PLACED, "placed"),
        (DirtyReasons::RAGDOLLS, "ragdolls"),
        (DirtyReasons::GORE, "gore"),
        (DirtyReasons::TERRAIN, "terrain"),
        (DirtyReasons::SLEEP, "sleep"),
    ];

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether every reason in `other` is in `self`.
    pub fn contains(self, other: DirtyReasons) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn insert(&mut self, other: DirtyReasons) {
        self.0 |= other.0;
    }

    pub fn bits(self) -> u16 {
        self.0
    }

    /// The names of the reasons in the set, in a fixed order.
    pub fn names(self) -> impl Iterator<Item = &'static str> {
        DirtyReasons::NAMES.into_iter().filter(move |(r, _)| self.contains(*r)).map(|(_, n)| n)
    }
}

impl std::ops::BitOr for DirtyReasons {
    type Output = DirtyReasons;
    fn bitor(self, other: DirtyReasons) -> DirtyReasons {
        DirtyReasons(self.0 | other.0)
    }
}

impl fmt::Display for DirtyReasons {
    /// `actor_parts+doors`, or `none`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("none");
        }
        for (i, n) in self.names().enumerate() {
            if i > 0 {
                f.write_str("+")?;
            }
            f.write_str(n)?;
        }
        Ok(())
    }
}

/// Counts of the refreshes [`crate::PhysWorld::update_queries`] ran since the world was
/// made. A map entry is added only the first time a combination of reasons is seen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefreshStats {
    /// Full collision detection passes.
    pub full: u64,
    /// Refreshes by the combination of reasons that called for them.
    pub by_reasons: BTreeMap<DirtyReasons, u64>,
}

impl RefreshStats {
    pub(crate) fn record_full(&mut self, reasons: DirtyReasons) {
        self.full += 1;
        *self.by_reasons.entry(reasons).or_default() += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_combine_and_print() {
        let mut r = DirtyReasons::NONE;
        assert!(r.is_empty());
        assert_eq!(r.to_string(), "none");
        r.insert(DirtyReasons::DOORS);
        r.insert(DirtyReasons::ACTOR_PARTS);
        assert!(r.contains(DirtyReasons::ACTOR_PARTS) && r.contains(DirtyReasons::DOORS));
        assert!(!r.contains(DirtyReasons::GORE));
        assert_eq!(r.to_string(), "actor_parts+doors");
        assert_eq!(r, DirtyReasons::ACTOR_PARTS | DirtyReasons::DOORS);
    }

    #[test]
    fn refreshes_are_counted_by_combination() {
        let mut s = RefreshStats::default();
        s.record_full(DirtyReasons::ACTOR_PARTS);
        s.record_full(DirtyReasons::ACTOR_PARTS);
        s.record_full(DirtyReasons::ACTOR_PARTS | DirtyReasons::BODIES);
        assert_eq!(s.full, 3);
        assert_eq!(s.by_reasons[&DirtyReasons::ACTOR_PARTS], 2);
        assert_eq!(s.by_reasons[&(DirtyReasons::ACTOR_PARTS | DirtyReasons::BODIES)], 1);
    }
}
