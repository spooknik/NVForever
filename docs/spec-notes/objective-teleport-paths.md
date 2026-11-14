# Objective teleport-link paths

[documented] [GetCurrentQuestObjectiveTeleportLinks](https://geckwiki.com/index.php/GetCurrentQuestObjectiveTeleportLinks)
returns an outer array of paths, each ending with its objective target. The SUP
[Alt interface](https://geckwiki.com/index.php/GetCurrentQuestObjectiveTeleportLinksAlt)
is a flat list of the current compass-marker references.

[observed: baseline 19c0f08a portable source] The current xNVSE handler returns a
flat array of targets and the Alt handler delegates to it. A consumer indexing a
path and then its first reference therefore receives the wrong shape. The existing
door graph already determines a bounded first-door marker with lock preference.

[design] Reuse that placed-door graph to retain each complete bounded route and
append its target. The nested interface returns one path array per shown target;
Alt returns the first marker in each path, or the target in the same place. Preserve
the existing first displayed, incomplete objective policy and read live lock state.
Cache the immutable placed-door graph for the Game; no per-callback rebuild is needed.

[observed: historical private repair, separate source] The repair ledger records
four array-subscript failures in an installed Modern Minimap map run, followed by a
held-map run without those script errors after the producer correction. That does
not establish complete map behavior, native implementation parity or a current-main
test result. The installed package version remains unqualified here.

[observed: authored fixture intent, not an execution result] The proposed CPU tests
use owned typed placed-reference metadata and exercise a nested same-place target,
a two-door route ending with its target, flat
Alt markers, no active/completed objective, cycles and unreachable paths. No installed
mod body, mesh, plugin or asset is included. Rendering and complete map acceptance
remain pending separately under the existing objective issue.
