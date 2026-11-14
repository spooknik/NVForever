# Temporary non-actor scene bases — initial scope

[documented] [TempCloneForm](https://geckwiki.com/index.php/TempCloneForm) creates
temporary base forms; its documentation and [extender save notes](ext-save.md)
state that these bases are not saved. This does not specify every property of a
reference placed from such a base; original-game lifetime controls remain needed.

[observed: portable 19c0f08a source] TempCloneForm stores a Game alias while
World::place admits only authored FormStore signatures. Thus an owned static
clone can report IsClonedForm but fail PlaceAtMe with unknown base. Merely using
the original ID for placement would lose the clone's identity and model override.
Drawing and collision independently resolve immutable records, so admitting the
ID alone would still leave the created reference without its chosen model.

[hypothesis: initial portable policy] Admit direct temporary clones of supported
non-inventory, non-actor scene bases through an explicit session-base descriptor.
Preserve the alias as RefState.base; use its known original for immutable type
metadata and its current model override for the created drawing/collision model.
Ordinary authored placement stays on its existing path. Reject unsupported clone
shapes explicitly rather than silently placing the original.

[hypothesis: save contract unresolved] Base nonpersistence does not establish a
safe policy for references created from that base. Silently omitting those references
or their tombstones can leave saved variables, quests, ownership and other references
dangling. That proposed omission policy was rejected in independent review before
production changes. Audit actual save/roundtrip boundaries and reference closure.
An interim candidate must explicitly refuse unsupported saves before enqueue/write,
after SaveGame callbacks, or implement a reviewed reconstructible contract. Neither
choice establishes native save parity or full playable B42 support.

[design] Owned controls must exercise the public source call, preserve the clone
base identity, route drawing and physics to the same selected model, and check
ordinary-reference independence plus explicit safe save/restore boundaries.
Test unknown and unsupported bases explicitly. Required engine/native checks
remain pending; no rendered/native run or complete B42 operation is established.

[hypothesis: remaining work] Clone-of-clone edits, post-placement model reload,
NPC/inventory cloning, all per-record traits, IDLE admission and complete save
parity are separate. They must not be implied by a nonzero placement return.

[design: reviewed interim guard] A checked Game snapshot inspects the retained
reference bases and deleted-created base tombstones against its clone table. Both
actual engine saving and the headless save/load boundary must use it. Engine
SaveGame callbacks run first; rejection then happens before serialization/enqueue,
leaving an existing file untouched. This deliberately refuses unsupported saves
instead of deleting reference closure. It is not a playable/native save solution.

[design: preview scope] Direct clones of STAT, MSTT and PROJ may use a session
descriptor, as inert model references only. Activator, talking-activator, door,
furniture, inventory and actor interactions are not admitted. PROJ admission does
not turn the created reference into a flying shot or establish complete B42 use.
The descriptor stores the authored original and the effective pre-placement MODL
override: clone-local first, then the original's session override. Explicit empty
is distinct from no override. Both created drawing and collision use that descriptor;
the selected path determines model kind, including an authored .spt replaced by NIF.
Post-placement model and script changes on both alias and its inherited original,
and clone chains, are explicitly refused in this candidate. Unrelated ordinary
placement and setters remain unchanged. The alias
itself stays the reference base. No screenshot, collision or consumer pass is yet
claimed; the implementation must remain a draft until the remaining validation
and persistence semantics are qualified.

[observed: source audit] ref_script/base_script do not inherit an alias's authored
original script. This preview therefore refuses originals or aliases with attached
scripts rather than create a reference that silently loses its script. General
script inheritance remains outside this initial admission; later script edits to
a placed alias or its original are also explicitly refused.

[design: practical save limit] A deleted clone reference keeps its base tombstone.
Consequently the guard still refuses saving after deletion, until an appropriate
load/reset replaces that state. This provisional safety policy is not a playable
persistence solution and must not be staged as a completed mod repair.

## Current-main source port

[documented: qualification scope] This provisional source port is based on exact
3dccb3c1c633e7d5eb06501de98dc580d7cda8a2 and is Part of #2391, not a
complete implementation of that issue. Earlier 19c core and selector controls are
historical evidence only. Compilation, tests, rendering and native controls on
this port have not run. Whole-save refusal, including deleted-created reference
tombstones, remains intentional until a complete persistence contract exists.
This is not a playable persistence or complete B42 acceptance claim.
