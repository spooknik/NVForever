# INI paths relative to Config

[observed: baseline 19c0f08a portable source] The INI resolver prepends `Config`
to a supplied file name. The layered reader and writer reject any remaining `..`
component. A request such as `../NVSE/Plugins/OwnedSibling.ini` therefore cannot
read an existing file inside Data, and equivalent in-Config spellings do not share
one path. The existing INI API rules are described in [INI API notes](jip-math-utility.md).

[observed: historical private repair, separate source] The repair ledger records
that the earlier sibling-path change removed the Tactapack NPC-perks configuration
warning and a HighRes configuration warning. A later review reproduced and corrected
a rooted-name alias introduced by prepending Config before rejecting rooted input.
Those aggregate runs are not execution evidence for this current-main port. Exact
package/version identity and complete perk or renderer operation are unqualified.

[design] Normalize only these Config-relative INI names lexically, starting with
Config and allowing a sibling that remains inside Data. Equivalent spellings then
use the same existing session/cache key. Reject a leading root, drive or stream
syntax, NUL, and a traversal above Data before the ordinary layered disk access.
An invalid name must not become an alias of a valid Config file. Ordinary file APIs
retain their strict plain-name checks. Writers keep the existing engine-home overlay
or session-memory behavior; they never gain permission to write into the game.

[observed: authored fixture intent, not an execution result] The proposed tests read
an owned INI through the public getter, check a session write and cached sibling
alias, preserve the original file, and exercise rooted/escaping names and lexical
path controls. They contain no installed scripts, configuration or assets.

[hypothesis: unmeasured boundaries] Physical symlink/junction containment remains
the existing reader/writer policy. This lexical change does not establish native
INI argument-binding parity, all settings behavior, mod dependency satisfaction,
NPC perk operation or complete mod compatibility. Current-base checks are pending.
