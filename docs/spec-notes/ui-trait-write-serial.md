# UI trait external-write serials

[observed: repository source at f30fb730bc6f263df98dbc60d0fa9437dd94654c,
`crates/fnv-ui/src/tree.rs`, `set_at`, `put_trait`, `update`, `write_changed`]
Successful value setters meet at `set_at`; equal values count as writes without
changing the value-dependency generation or making a clean tree dirty. XML
loading/injection/includes/templates insert or replace traits through `put_trait`
instead. Expression evaluation updates dependency state separately.
`write_changed` intentionally does nothing when its own last value is unchanged,
including after a script's intervening write.

[design] Give each accepted external value write and each XML trait insertion or
replacement an opaque, Ui-local nonzero serial, including equal-value writes.
Store it separately from value-change/expression generations and diagnostics.
The read-only accessor takes the full live TileId and case-insensitive trait name;
missing/dead/wrong-tree targets return no serial. Trait vector indices and current
values are not ownership identities. A removed/recreated trait gets a fresh
serial; an unrelated trait's write does not change it. No log/map of past writes
is retained, and serial bookkeeping adds no allocation.

[design] The Ui clock starts at zero and increments with checked u64 arithmetic.
On exhaustion, accepted legacy writes continue with their existing value and
expression behavior but that written/replaced trait has no serial. A previously
issued serial is never reused or wrapped. The serial is not persisted across Ui
replacement; the full TileId's tree/slot generation remains part of identity.

[design] Expression evaluation does not advance the external-write serial. An
external setter targeting an expression trait does advance it and retains the
existing expression/hold-until-source-change behavior. The accessor may return a
serial for an expression trait: a serial records writes, not writable authority.
`write_changed` early returns issue no serial. No setter or expression semantics
are changed to manufacture ownership.

[design] Eight authored test groups cover accepted setters/equal writes,
independent traits/case aliases, changed-only wrappers, XML/include/template
replacement, expression replacement, expression evaluation and direct overrides,
removal/slot/tree identity, and counter exhaustion with continued legacy values.
Fixtures use independently authored XML and values, not game/mod files.

[observed: local qualification on validation checkout
54830c0d09c912201de0687f37dc2a236b087fd1, pinned Rust/Cargo 1.99, iter profile,
locked/offline, one build job and one test thread] The original f30 fnv-ui module
and support/fixture continuity passed all 82 existing library tests. Importing
only the two candidate Rust files passed all 90 tests, including the unchanged
82 and eight new controls, with none failed, ignored or filtered. Focused fnv-ui
library/unit-test clippy passed with `-D warnings`. This was not a pristine whole
f30 workspace run. Original f30 has no serial API; no old-version failures are
inferred for the new tests. Full release/workspace, smoke, allocation-counter and
live renderer/native checks were not run for this prerequisite.

[design] This metadata is only a prerequisite for a possible plain-sink lease.
There is no transaction, restore/lease API, local-map provider, recipient adapter,
session gate, projected UI port or renderer change here. Such a service must
separately require plain expressions, complete TileId/session/subtree identity,
all-sink conditional publication and conditional restoration, serial-capacity
preflight, checked budgets and an owned resource lifecycle. No native DLL parity
or live held-map repair is asserted.
