# Quest sweep: first results (2026-11-14)

`fnvtool quest sweep` (docs/design/quest-sweep.md, issue #2365 item 5) over the owner's
install, on the branch of the pull request that adds it. Every figure is [observed] from
the command's output. FalloutNV.esm alone: about 5–13 s (most of it building `GameData`);
the ten masters: about 21 s.

## Counts

| Check | FalloutNV.esm | ten masters |
|---|---|---|
| 1a conversations with no way out | 0 | 0 |
| 1b actors with no Goodbye shown open to them | 7 | 9 |
| 2 control holds (units that take movement) | 116 | 164 |
| 2 … released in the same block | 3 | 3 |
| 2 … released elsewhere, nothing risky on the way | 34 | 50 |
| 2 … waits (package, line, trigger, timer, actor…) | 71 | 89 |
| 2 … of those, waits on more than a timer | 17 | 23 |
| 2 … no release found within 4 steps | 8 | 22 |
| 3 stages set and tested | 652 | 871 |
| 3 stages only hard-to-reach things set | 14 | 14 |
| 4 Requires Key doors without a key | 49 | 96 |
| 4 … opened only by a script | 34 | 72 |
| 4 … never opened | 15 | 24 |

1a reads 0 because the "Let's talk about something else." topics carry only shared lines
(INFC/INFX, GREETING lines with no choices); the first run, which left shared lines out,
gave 100 false traps.

## The playtester reports of 2026-11-13

| Report | Flagged? | Where |
|---|---|---|
| #2453 Ramos walks off, controls stay off | **yes** | check 2, `waits`: INFO 0x000A3CE0's end script takes the controls and writes `HVRamosTargetDialogTriggerREF.timer`/`dofade`; the only release is that trigger's `OnTrigger player` block (SCPT 0x0015E57A) behind the variable and a timer. The player must stand in the trigger for the controls to come back. |
| #2454 Ramos silent on the intercom, movement locked | **probably** [hypothesis: the report has no log] | check 2, `waits`: McNamara's INFO 0x0016329B (HVMcNamRangerYes, sending the player out with Ramos) takes the controls; the release is HVMcNamaraHardinConvoTriggerREF's script behind `outwithramos` and a timer. The same line unlocks the bunker doors 0x000DA1C6, 0x000E298C, 0x000AD632. |
| #2451 Veronica never walks to the intercom | no | The stall is a Dialogue package aimed at a talking activator that the AI does not carry out: nothing in the data is wrong, so no static check fires. The bunker doors are not Requires Key without a key, so check 4 does not list them. |
| #2445 Chauncey never comes to the steam room | no | VMS18's stages other than 255 are not set by a `SetStage` the sweep sees as hard to reach; the meeting hangs on a scheduled package. Check 3 lists only stages whose every setter is a package result script; this one is not set that way. |
| #2433 Orris stands still after one shot | no | A Use Weapon / escort package that the AI ends early: runtime behaviour. |
| #2424 Victor's "Stay Here" ends talking to him | no | A message box answered and the activation lost: outside the four shapes. |

Also from the issue's text: Hardin's training (#2309) is flagged (check 2, `waits`): INFO
0x001633C9 restrains the player, writes `HVMcNamaraHardinConvoTriggerREF.hardintraining`;
the release found is INFO 0x001633C0 via `gives package HVHardinPowerArmorTrainingPackage
(says HardinPowerArmorTraining)`, tags `package`, `dialogue line`. McNamara's version
(INFO 0x001638DA) the same. REPCONN's `REPCONMidToTopDoor` is in check 4, opened by INFO
0x000CD87E's end script (Chris Haversam's line, heard through the intercom).

So of the six reports two have the shapes this sweep looks for, both found; the other four
are AI or menu behaviour at run time, which is what the watchdog (#2367) and the quest
pilot (#2401) are for.

## Worth a look

- **#1595 Julie Farkas is not flagged.** Her voice type FemaleAdult11 (0x00176578) is in
  the Generic quest's dialogue conditions, and that quest's GOODBYE line 0x00177392 (text
  "...", Goodbye flag, Conversation type, no conditions of its own) is open to her by the
  data. The comment on #1595 says every generic GOODBYE line fails for her. Either the
  engine leaves out that line for another reason (its type, the quest's other conditions
  at run time) or the comment's reading was wrong; observer check 12 asks how the original
  lets the player out.
- Check 1b's nine actors (RaulTejada, VFSArcadeGannon, Rex, FortMelody, VRRCPapaKhan,
  VES05NightkinTWeedRancher, VStreetSecuritronMk2 in the base game) have Goodbyes only
  behind a condition the sweep cannot decide (mostly the Followers quest's teammate lines,
  0x000A53F4): companions and robots whose leave-taking depends on state. NVForever lets
  the player Tab out of a topic list, so none traps a player here.
- Check 3, the 14 stages: VCG02 30 and 40 (Sunny's travel packages' end scripts), VCG00 14
  (Benny's travel to the player), VVault21Tour 20 (Sarah's end-of-tour package), VMQTops
  45 (`VTopsBennysPassageTrigger` 0x0013F5B7 has no navmesh within reach), VMS15 40 (the
  ballast deployer starts disabled and nothing enables it by script: an enable parent or
  a stage elsewhere), two Legion dropbox stages (containers that start disabled), three
  trigger scripts never placed (test content), and three stages tested but never set
  (CGTutorial 50, VMS11 70, 80). The package ones are the shape of #2445: the quest goes on
  only when an AI package finishes.
- Check 2's 23 holds that wait on more than a timer (ten masters) are the list to run the
  watchdog against first; the other `waits` are fades whose release counts seconds.

## Next

- The quest pilot (#2401) can take the stage list and the control holds as places to
  stop and check that a player gets out.
- Check 2 does not follow global variables or `OnLoad` blocks of other cells; most of the
  `unreleased` holds are there (VEndingScript, the Hoover Dam assassination lines).
