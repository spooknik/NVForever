# Required animation-event codes are not actor wildcards

[documented] The public API descriptions for
[SetOnAnimActionEventHandler](https://geckwiki.com/index.php?title=SetOnAnimActionEventHandler)
and [SetOnPlayGroupEventHandler](https://geckwiki.com/index.php?title=SetOnPlayGroupEventHandler)
require an action code (0 through 13) or animation-group keyword after the actor
filter. An empty actor form list means any actor for the specified action/group.
The callback receives the numeric action/group, with the actor as its receiver.
See also `jip-events.md` sections 1 and 2 and the existing function catalogue.

[observed: portable source f30fb730] `jip_matches` applies `jip_wildcard` to every
slot. That helper treats numeric zero as unrestricted, including the required
animation argument. A handler registered for action zero therefore matches a
different numeric action. Numeric/numeric `filter_matches` already uses equality.

[design] Give only AnimAction and PlayGroup's argument-zero slots an explicit
`ExactArg` classification. These slots skip the wildcard shortcut and keep the
existing comparison. Actor filters and unrelated events retain their policies.
PlayGroup's numeric zero fixture checks the portable dispatch representation;
keyword normalization and native keyword/casing behavior are separate work.

[observed: private focused validation, 2026-11-13] The authored fixture registers
source lambdas through the real API and dispatches numeric events through
`Game::jip_event`. A private engine-workspace test member compiled the exact
unchanged original fixture file against the real fnv-game library, resolving the
same schema root. The actual validation checkout was `54830c0d`, with the affected
f30 module/common/catalogue and original lock continuity checked; this was not a
pristine whole-f30 workspace or canonical fnv-game Cargo-target execution.

[observed: same-fixture pair] Baseline: two tests passed and three failed at the
intended callback-count assertions. Failed tests stopped at their first assertion,
so not every later statement ran on baseline. Replacing only JIP production with
this change gave five passes, zero failures/ignored/filtered tests. The controls
cover nonzero neighbors, actor-specific and empty-list filters, received values
and receivers, and unchanged actor/unrelated-slot wildcard policies.

[observed: qualification limits] Private hosted fixture Clippy passed with
`-D warnings`; production library lint was not qualified. A separate external
dependency lint attempt reported fnv-game Fresh and is not counted as production
lint. The initial direct external-package test was rejected before compilation
and ran zero tests. Canonical root/engine targets, full release/strict suites,
smoke scenarios and native behavior remain unchecked by this focused proof.

This change provides no automatic animation-event producer, cloned-list repair,
registration provenance, native plugin loading or complete installed-mod support.
