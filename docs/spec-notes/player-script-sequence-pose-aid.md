# Owned player sequence pose observer

[design: #2272] `player-script-sequence-pose-release-owned` is an optional,
registered GAME aid. Its GAME execution is **NOT RUN**; Rust/CPU validation is
recorded separately in the PR. It reads the actual
first/third player rigs in `PostUpdate` after `TransformSystems::Propagate`;
it does not set a request, a pose, a clip cache, a layer or a transform. The
scenario's ordinary console calls start/stop the sequence. Missing rigs, missing
tracks, missing required inputs or a missed phase cannot print all five PASS
lines. These observations qualify neither native parity nor pixels.

## Owned input gate

Set `NVF_PLAY_TEST_SEQUENCE_POSE` to an absolute private JSON manifest path.
The smoke runner inherits it and skips the scenario when it is absent. Use a
private Data view containing **authored, non-game-derived** KFs at
`meshes/qa/sequence2395/owned_underlying.kf` and
`meshes/qa/sequence2395/owned_pose.kf`. No binary assets are supplied here.
The exact case-sensitive bone name must exist in both real player skeletons.
The underlying KF must hold that bone at `base`, with no other tracks; the high
KF must have one linear translation track from `clip_start` to `clip_end`,
start 0, stop 1, frequency 1, cycle 2 (clamped), with no root accumulation or
other channels. The observer checks the loaded high clip's timing/cycle fields,
but does **not** certify the provenance or exact channel bytes of external KFs.
Those authored inputs and their hashes need independent review before a run.
Choose a bone unaffected by camera, aiming, locomotion and other installed
animation writers; otherwise the numeric oracle should fail, not be relaxed.

Example manifest shape (replace the bone with the exact owned fixture target):

```json
{"schema":"nvf.sequence-pose-observer/1",
 "path":"qa\\sequence2395\\owned_pose.kf",
 "bone":"Owned fixture target in both skeletons",
 "base":[10,0,0],"clip_start":[20,0,0],"clip_end":[40,0,0],
 "tolerance":0.001}
```

The bounded 16 KiB manifest admits only finite vectors representable as f32 and
a finite tolerance from 0 through 0.01. It provides the independent numeric
oracle, not observed values copied after the run. The proposed 25 second budget
is unmeasured. Do not bypass the input gate using unreviewed installed clips.

## Measurements and limits

The first observation requires both lower requests and their actual animator
tags. It checks both local translations against `base` and world translations
against each actual parent's propagated transform. It retains their entities
and the third rig's original local/world positions as preservation controls.
The high request's serial must remain stable and its tag must occur only in
first person. The active phases require actual clock windows [.25,.5) and [.75,1), and
the second must measurably change both first local and world translations. A
missed window cannot be replaced by the same clamped endpoint twice. At those
two actual request-clock phases, expected translation is the
independent authored linear track sampled at that measured clock (clamped to
0..1). This avoids assuming that a render frame lands exactly on .25 or .75.
A fade observation waits beyond the actual stop time, requires a genuine
intermediate actual layer ease, and checks that share reaches the ECS output.
It does not independently qualify core/render clock alignment or fade duration;
the CPU fixture checks the explicit quarter-fade timing. Release requires the
request absent, the high tag absent, both lower tags retained and `base`
restored. Every accepted phase logs the measured clock, entities and four
actual local/world vectors before its PASS line. Identity replacement or a
numeric/tag mismatch produces FAIL and prevents subsequent PASS lines.

The lower layer in this GAME aid is another owned script sequence, not the
ordinary locomotion idle. The separate prepared-consumer CPU fixture checks
restoration to its authored ordinary underlying clip. This aid does not prove
asynchronous KF availability, the entire normal schedule, renderer pixels,
asset parser correctness, native timing, morph/additive policies or a full mod.
A future execution must retain all inputs, hashes, logs and actual failure
verdicts; registration here is not an execution result or Ready approval.
