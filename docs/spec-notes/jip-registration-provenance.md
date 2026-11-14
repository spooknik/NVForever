# JIP registration provenance diagnostics

[observed: source c3594712138c412f00f09a46c9e4a5a4173379c9]
`Exec::event_call` still knows the executing Compiled unit and RunCtx when a
source registration reaches Game. Its existing calling_plugin API distinguishes
runtime diagnostic labels from plugin ownership. A later unknown-lambda failure
may have no callable declaration metadata; its registrar cannot be reconstructed
from the callable or a generic handler-kind string.

[design] Capture bounded metadata at this actual registration boundary: unit
diagnostic key, actual calling_plugin result, owner form, available source-file
origin, stable compiled callable ID and RunCtx self/action/quest IDs. Retain no
Compiled unit, source body, lambda or callable reference for this purpose. These
are registrar fields, never a guess at the callable's declaration or installed mod.
Direct Game event-call APIs have no verified executing unit and explicitly report
unavailable provenance; their arbitrary sender string is not promoted to a plugin.

[design] Keep at most 192 Unicode scalar values per stored text field and escape
controls only when an admitted failure is logged. Truncation is explicit. Store
metadata by the actual active registration sequence; duplicates keep the first
registration's metadata, and removal drops only removed sequences. Dispatch
snapshots bounded Arc metadata with its existing handler snapshot before callbacks
run, so nested removal cannot erase the attempted registration's origin. No
handler equality, order, result, callable retention or GC rule changes.

[design] CompileScript's normalized Data-relative file origin is captured at
successful compilation, keyed by that stable compiled callable ID. Retain only
the latest 128 compiled-unit origins (including dynamic no-file entries), evicting
the lowest/oldest ID. Force-recompiled old IDs keep their origin only while within
this explicit diagnostic bound; an evicted unit remains executable but any later
registration from it reports origin not retained. Already captured active
registration metadata remains intact. No lambda/unit lifetime is extended and
no filename is reverse-guessed from the current path cache. ScriptRunner source
paths come from their actual SR unit label; query/console/dynamic sources remain
distinct and never become calling-plugin identities.

[design] Only warnings admitted by the existing handler error/dedup gate gain
structured registrar fields alongside event-kind, sequence and raw callable
context. No successful-registration/per-frame warning is added. Original
ScriptError payloads and reporting frequency remain intact. Fields and capture
are session diagnostics, absent from saves and JIP-kept state.

[design] Authored controls must use actual source file/SCPT/query/nested calls,
real registration/removal and structured failure diagnostics, with valid neighbors,
duplicate and nested-removal controls, force-compilation history, eviction and
bounded/control text. No installed mod scripts or native implementations are used.
Source-only preparation is not a passing test or one-run live attribution result.
