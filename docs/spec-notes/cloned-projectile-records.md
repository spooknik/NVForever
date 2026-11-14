# Temporary cloned projectile record access

[documented] [TempCloneForm](https://geckwiki.com/index.php/TempCloneForm) returns a
temporary base-form clone; its form is not persisted in a save. The projectile
setter argument/field contract is described in [JIP batch 7](jip-records-7.md),
including numeric speed trait 2, light and muzzle-flash path. Public references:
[SetProjectileTraitNumeric](https://geckwiki.com/index.php/SetProjectileTraitNumeric),
[SetProjectileLight](https://geckwiki.com/index.php?title=SetProjectileLight).

[observed: baseline 0b16bea3 portable source] `Game::clones` retains the original
identity. `nvse::record_value` already reads clone overrides and inherited records,
but the VM host's separate `record_field` path does not use that helper. Three PROJ
setters check only `GameData::sig(clone)`, where no loaded record exists. This can
silently return default values or skip writes despite a valid temporary clone.

[implementation] Delegate VM record reads to the existing clone-aware helper.
Validate the inherited signature for only SetProjectileTraitNumeric,
SetProjectileLight and SetProjectileMuzzleFlash, while retaining the exact clone ID
for writes. The owned tests exercise inherited speed, zero-valued overrides,
clone-local light set/clear and muzzle path, invalid signature/trait controls,
original independence and a fresh Game. Other typed setter guards stay as before.

[observed: earlier owned portable qualification, separate source] The public three-
test baseline failed; changing only the setter guards left two passes and one
inherited-getter failure. The combined isolated change passed those tests and lint.
That aggregate source is historical evidence, not current 0b16 execution proof.

[hypothesis: unmeasured native policy] Reusing the current helper retains its live
inheritance policy. Native copy-versus-inheritance after editing an original,
save/reload, scene placement, rendering, projectile physics and complete consumers
remain unqualified. A successful state getter is not graphical acceptance.
