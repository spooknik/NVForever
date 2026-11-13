# GetIsFormType source-call boundary

[documented] The existing `schemas/script/functions.json` catalogue entry for
[GetIsFormType](https://geckwiki.com/index.php?title=GetIsFormType) describes a
reference predicate whose argument is a four-character record signature, such as
ACTI, CONT or CREA. This is separate from GetObjectType's numeric result.

[observed: baseline 19c0f08a portable source] The catalogue declares the function,
but the named-call dispatcher has no handler. `functions::base_of` already resolves
placed references and temporary inventory references to their base forms. The
existing `nvse_other` base helper also leaves a direct base-form argument available
for signature lookup when it is not a placed reference.

[observed: candidate design; current execution pending] Resolve the calling subject
through that helper and compare its base signature with a four-byte argument.
Return false for missing subjects, wrong-length arguments or different signatures.
The isolated owned fixture checks an ACTI placed reference, a WEAP base and an actual
script-created inventory reference, plus mismatched/invalid argument controls. It
contains no installed mod text or game data. Its current-base execution is pending.

[hypothesis: native casing not measured] The retained portable policy compares ASCII
signatures without case. The lowercase authored control tests that portable policy,
not original-game parity. This named-call patch adds no native opcode mapping and
does not establish event-handler delivery, clone signatures or complete Quick Drop
behavior. Earlier aggregate-primary small-library results remain historical only.
