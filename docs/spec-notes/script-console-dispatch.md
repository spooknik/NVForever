# Script Console expression dispatch

[documented] [Console](https://geckwiki.com/index.php/Console) runs a console line
from a script. The existing interactive statement path is described in
[console design](../design/console.md).

Baseline 19c0f08a source: The script Console wrapper tokenizes
the line and tries console/function word dispatch. It does not route an assignment
such as `OwnQuest.value += 1` through the existing statement compiler, even though
that compiler path exists for interactive lines. The authored fixture uses synthetic
quest/global state rather than installed script text.

[design] Select a statement or word-dispatch path before invoking a host function.
A failed host call must not be retried through another parser. Keep console-only
commands and literal classic assignments on their existing selected-reference path;
parenthesized and compact expression assignments use the statement compiler.
Operators inside quoted command arguments do not select expression dispatch.

[design] Cache identical script Console sources per Game under the earlier repair's
16 KiB source and 256 distinct-source budgets. Existing entries still run when the
count budget is reached. These are portable safety policies, not observed native
limits. Use console/no-plugin identity, live game state and existing diagnostics;
syntax/runtime failures are reported without ending the wrapper's caller.

Historical private regression context (not an original-game observation): The initial expression
repair received mandatory dev8 corrections for compact classic expressions and
pre-dispatch without side-effect retries. Its eight script regressions and lint
were reported passing on that aggregate source. No historical counts transfer to
this current-main draft. Compile-count instrumentation, exhaustive optional syntax
parity and complete consumers remain unqualified.

[design] Maintainer review of710c333 (PR2397 comment6875026328) identifies a
compatibility regression: signed numeric command words must not enter the bounded
expression cache. A declared string argument retains the word reader's unquoted
path/name spelling. Resolved form words also remain word arguments. Parenthesized
or arithmetic arguments and assignments still select compilation before execution;
runtime failures never trigger a second dispatch attempt.

Authored source controls retain the original eight regressions and add 300 changing
negative SetPos/SetAngle pairs, the complete remaining expression budget, word calls
after that budget, and an unquoted hyphen/slash argument with a single host refusal.
At initial source preparation these controls were authored; subsequent source-bound
execution results are recorded in the PR.
