# Explicitly cleared cloned form lists as JIP actor wildcards

- [documented] JIP's actor-or-list filter accepts a form list, and an empty list
  means any actor for SetOnPlayGroupEventHandler and SetOnAnimActionEventHandler.
  Sources: public API catalogue `schemas/script/functions.json` and
  [JIP event notes](jip-events.md), section 1; public pages
  <https://geckwiki.com/index.php?title=SetOnPlayGroupEventHandler> and
  <https://geckwiki.com/index.php?title=SetOnAnimActionEventHandler>.
- [documented: repository source f30fb730] TempCloneForm records an alias/original
  relation. ListClear materializes an empty session list keyed by the alias.
  The current JIP wildcard gate checks only the alias's authored signature, so it
  cannot recognize that explicitly cleared cloned FLST as a list. The ordinary
  membership comparison then has no members and the handler silently matches no
  actor. This is a source-proven mismatch; no native execution is claimed.
- [design] Resolve the declared alias chain for the wildcard's type check, without
  changing callback registration, order, argument delivery, removal or membership
  matching. Honor the actual materialized list state, not the original's members,
  when ListClear/ListAddForm has supplied one. Wrong record types and unresolved
  alias origins must not become empty-list wildcards.
- [design] An unmaterialized clone must not become a universal filter merely
  because the current ordinary list reader does not inherit its original's
  contents. Full copied-list membership and the timing of original/clone edits
  are separate work. This candidate must preserve the nonempty-original negative
  boundary instead of claiming that feature is implemented.
- [design] A local, allocation-free alias walk is bounded by the actual clone-map
  size plus the terminal lookup. It refuses an unresolved or cyclic graph rather
  than introducing an unbounded walk in event dispatch. It adds no arbitrary
  clone-depth threshold and does not rewrite the global unclone API. Public
  TempCloneForm's ordinary acyclic chains keep their full type-resolution path;
  this is not a claim of an observed cyclic mod graph.

## Owned controls and limits

Authored controls register actual source-compiled AnimAction handlers, fire the
same actor/action event through Game::jip_event, and observe their callbacks beside
a valid neighboring registration. They compare ordinary empty, cloned-cleared
empty, a longer alias chain, materialized nonempty clone and wrong-type filters.
An untouched clone of a nonempty original must not become universal. Fixtures
contain no installed mod scripts or game assets.

[design] Registration must return success through its real source API. A different
literal action must not run either handler, and callback counts must distinguish
the selected registration from its valid neighbor. Tests do not expose private
registration fields just to inspect them.

This does not identify an unknown-lambda callback, implement copied-list
membership, repair cloned combat-style assignment, change saves, or establish
complete mod/native compatibility. Fixture presence alone is not a passing result.

[observed: private CPU qualification with pinned Rust/Cargo 1.99, iter profile,
one build job and test thread, locked/offline] Exact f30 original/candidate
`jip_events.rs` and the identical final six source fixtures/support were used with
real Game in a private test host within validation checkout 54830c0d09c912201de0687f37dc2a236b087fd1.
Original production passed four tests and failed two at assertions; candidate
passed all six, with none ignored or filtered. The failed tests stopped at their
first failing assertions, so later inner assertions are not claimed executed.
Strict lint of the private fixture target passed. This was neither the canonical
test target nor a pristine whole-f30 workspace run. Production-library lint,
broader suites, automatic event production and native/game/GPU acceptance were
not qualified. The separate zero-animation-code change was absent in both phases.
An earlier invalid PlayGroup/private-field fixture was corrected before any
build and remains separately retained as unexecuted preparation evidence.

[design] The fixture uses AnimAction's literal integer action filter to keep
PlayGroup keyword interpretation outside this repair. It does not exercise an
automatic animation producer; manually dispatching a real Game event does not
establish that playing an animation will produce that event in the client.
