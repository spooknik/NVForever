"""The probe sets behind docs/spec-notes/ext-unlisted-rest.md, batches 2-5 (#2326).

Sets json1, json2, knv1-knv3, rest4 and rest5, and the jobs that ran their risky rows one
per launch (json2r0-r5, knv2r0-r2, rest4r0-r5, rest5r00-r11: a call that stopped the
game took only its own launch with it). Each was run once, unattended, by nvf_capture v10
in the Blue Moon sandbox's stock game from the owner's save nvfairport on 2026-11-13
(xNVSE 6.4.9; JIP LN 57.30, ShowOff 184, SUP 855, kNVSE 37, JohnnyGuitar 528, One111 120,
ITR 20203 by their nvse.log load lines). The rows below rebuild those jobs' steps exactly
(tools/capture/tests/test_unlisted_rest_probes.py holds the step count of each), save one
thing: a rerun's first row (json2's cache, rest4's and rest5's placing of Doc Mitchell)
had its set's label in the run and carries the job's name here (`json2.json2r0.cache`,
`rest4.rest4r0.place`), so that ids stay unique across sets; its golden row is renamed
the same way when collected. The
goldens are private (`<goldens>/jip/<set>.json`); `jip_probes.py steps` writes the job
files into tools/capture/jip-probes/, which the lab's candidate runs.

Prepared files: json1 and json2 read `nvfprobe.json` in the game folder (json1 also
`Data\\nvfprobe.json` and `Data\\nvfprobe2.json`, json2 `nvfprobe3.json`), written just
before the run with the contents in JSON1_FILE and JSON2_FILE. Launches that stopped the
game before their last row: json1 after 37 of 52 rows, knv1 after 60 of 64, json2r0 and
json2r1 after their first, knv2r0 and knv2r1 before any (the capture's run report).

`shape_jobs` writes the compile-only argument-shape run of §1: every name of SHAPE_NAMES
with each of SHAPE_PATTERNS inside `if 0`, in jobs of 150 steps (4,715 steps, shp00-shp31).

Standard library only; jip_probes.py imports this.
"""
import json
import os

# The files json1 and json2 found [the run's own inputs, not game data].
JSON1_FILE = {"num": 1.5, "str": "text", "arr": [1, 2, 3], "obj": {"inner": 4}}
JSON2_FILE = {"num": 1.5, "str": "text", "arr": [1, "two", 3], "obj": {"inner": 4, "deep": {"x": 5}}}


def _rows(prefix, header, rows):
    """(label, body, text) rows -> jip_probes' (id, body, text, expected, page)."""
    return [(f"{prefix}.{k}", header + b, t, None, "") for k, b, t in rows]


def _json1():
    H = "float fA\nstring_var s\narray_var aA\nBegin Function { }\n"
    F = "nvfprobe.json"
    rows = [
        ("cached.before", f'SetFunctionValue (JSONIsFileCached "{F}")', False),
        ("cache", f'SetFunctionValue (JSONCacheFile "{F}" 0)', False),
        ("cached", f'SetFunctionValue (JSONIsFileCached "{F}")', False),
        ("cache.data", 'SetFunctionValue (JSONCacheFile "Data\\nvfprobe2.json" 0)', False),
        ("cached.data", 'SetFunctionValue (JSONIsFileCached "Data\\nvfprobe2.json")', False),
        ("cache.missing", 'SetFunctionValue (JSONCacheFile "nvfnosuch.json" 0)', False),
        ("tostring", f'SetFunctionValue (JSONToString "{F}")', True),
        ("tostring.data", 'SetFunctionValue (JSONToString "Data\\nvfprobe2.json")', True),
    ] + [(f"type.{k}", f'SetFunctionValue (JSONGetValueType "{F}" "{k}" 0)', False)
         for k in ("num", "str", "arr", "obj", "nokey", "obj/inner", "obj.inner", "arr/1", "arr[1]")] + [
        ("type.flag1", f'SetFunctionValue (JSONGetValueType "{F}" "num" 1)', False),
        ("arr.get", f'let aA := JSONGetValueArray "{F}" "arr" 0\nSetFunctionValue (sv_Construct "%g %z" (ar_Size aA) (TypeOf aA))', True),
        ("arr.values", f'let aA := JSONGetValueArray "{F}" "arr" 0\nSetFunctionValue (sv_Join aA ",")', True),
        ("arr.obj", f'let aA := JSONGetValueArray "{F}" "obj" 0\nSetFunctionValue (sv_Construct "%g %z" (ar_Size aA) (TypeOf aA))', True),
        ("arr.num", f'let aA := JSONGetValueArray "{F}" "num" 0\nSetFunctionValue (sv_Construct "%g %z" (ar_Size aA) (TypeOf aA))', True),
        ("setfloat", f'SetFunctionValue (JSONSetValueFloat "{F}" "num" 7 0)', False),
        ("after.setfloat", f'SetFunctionValue (JSONToString "{F}")', True),
        ("setfloat.new", f'SetFunctionValue (JSONSetValueFloat "{F}" "newnum" 8 0)', False),
        ("setfloat.flag1", f'SetFunctionValue (JSONSetValueFloat "{F}" "num" 9 1)', False),
        ("setfloat.five", f'SetFunctionValue (JSONSetValueFloat "{F}" "num" 10 0 0)', False),
        ("after.setfloat2", f'SetFunctionValue (JSONToString "{F}")', True),
        ("setstring", f'SetFunctionValue (JSONSetValueString "{F}" "str" "changed" 0)', False),
        ("after.setstring", f'SetFunctionValue (JSONToString "{F}")', True),
        ("appendfloat", f'SetFunctionValue (JSONAppendValueFloat "{F}" "arr" 4 0)', False),
        ("appendstring", f'SetFunctionValue (JSONAppendValueString "{F}" "arr" "five" 0)', False),
        ("after.append", f'SetFunctionValue (JSONToString "{F}")', True),
        ("erasearray", f'SetFunctionValue (JSONEraseArrayElement "{F}" "arr" 0 0)', False),
        ("after.erasearray", f'SetFunctionValue (JSONToString "{F}")', True),
        ("clearkey", f'SetFunctionValue (JSONClearKey "{F}" "obj" 0)', False),
        ("after.clearkey", f'SetFunctionValue (JSONToString "{F}")', True),
        ("erasekey", f'SetFunctionValue (JSONEraseKey "{F}" "str" 0)', False),
        ("after.erasekey", f'SetFunctionValue (JSONToString "{F}")', True),
        ("fromstring", 'SetFunctionValue (JSONCacheFileFromString "{\\"a\\":1}" "nvfmem.json")', False),
        ("fromstring.cached", 'SetFunctionValue (JSONIsFileCached "nvfmem.json")', False),
        ("fromstring.tostring", 'SetFunctionValue (JSONToString "nvfmem.json")', True),
        ("fromstring.other", 'SetFunctionValue (JSONCacheFileFromString "nvfmem2.json" "{\\"b\\":2}")', False),
        ("fromstring.other.tostring", 'SetFunctionValue (JSONToString "nvfmem2.json")', True),
        ("close.0", f'SetFunctionValue (JSONCloseFile "{F}" 0)', False),
        ("cached.after.close", f'SetFunctionValue (JSONIsFileCached "{F}")', False),
        ("recache", f'SetFunctionValue (JSONCacheFile "{F}" 0)', False),
        ("after.recache", f'SetFunctionValue (JSONToString "{F}")', True),
        ("setfloat.save", f'SetFunctionValue (JSONSetValueFloat "{F}" "num" 11 0)', False),
        ("close.1", f'SetFunctionValue (JSONCloseFile "{F}" 1)', False),
        ("close.data.1.1", 'SetFunctionValue (JSONCloseFile "Data\\nvfprobe2.json" 1 1)', False),
        ("cached.after.close1", f'SetFunctionValue (JSONIsFileCached "{F}")', False),
    ]
    return {"json1": _rows("json1", H, rows)}


def _json2():
    H = "float fA\nfloat fB\nstring_var s\narray_var aA\narray_var aB\nBegin Function { }\n"
    F = "nvfprobe.json"
    EL = ('let s := ""\nforeach aB <- aA\n  let s += sv_Construct "%z=%z;" (TypeOf aB["key"]) (TypeOf aB["value"])\nloop\nSetFunctionValue s')
    rows = [
        ("cache", f'SetFunctionValue (JSONCacheFile "{F}" 0)', False),
    ] + [(f"type.{k}", f'SetFunctionValue (JSONGetValueType "{F}" "{k}" 0)', False)
         for k in ("/obj/inner", "obj/deep/x", "/arr/1", "/num", "", "/")] + [
        ("arr.types", f'let aA := JSONGetValueArray "{F}" "arr" 0\n' + EL, True),
        ("arr.first", f'let aA := JSONGetValueArray "{F}" "arr" 0\nlet fA := aA[0]\nSetFunctionValue fA', False),
        ("arr.second", f'let aA := JSONGetValueArray "{F}" "arr" 0\nlet s := aA[1]\nSetFunctionValue s', True),
        ("obj.types", f'let aA := JSONGetValueArray "{F}" "obj" 0\n' + EL, True),
        ("obj.keys", f'let aA := JSONGetValueArray "{F}" "obj" 0\nSetFunctionValue (sv_Join (ar_Keys aA) ",")', True),
        ("num.types", f'let aA := JSONGetValueArray "{F}" "num" 0\n' + EL, True),
        ("num.keys", f'let aA := JSONGetValueArray "{F}" "num" 0\nSetFunctionValue (sv_Join (ar_Keys aA) ",")', True),
        ("arr.cache", f'let aA := JSONGetValueArray "{F}" "arr" 1\nSetFunctionValue (ar_Size aA)', False),
        ("set.cache", f'SetFunctionValue (JSONSetValueFloat "{F}" "num" 2 1)', False),
        ("after.set.cache", f'SetFunctionValue (JSONToString "{F}")', True),
        ("type.new.cache", f'let fA := JSONSetValueString "{F}" "added" "yes" 1\nSetFunctionValue (JSONGetValueType "{F}" "added" 1)', False),
        ("type.new.file", f'SetFunctionValue (JSONGetValueType "{F}" "added" 0)', False),
        ("append.cache", f'SetFunctionValue (JSONAppendValueFloat "{F}" "arr" 4 1)', False),
        ("after.append.cache", f'SetFunctionValue (JSONToString "{F}")', True),
        ("append.string.cache", f'SetFunctionValue (JSONAppendValueString "{F}" "arr" "five" 1)', False),
        ("erase.elem.cache", f'SetFunctionValue (JSONEraseArrayElement "{F}" "arr" 0 1)', False),
        ("after.erase.cache", f'SetFunctionValue (JSONToString "{F}")', True),
        ("clear.cache", f'SetFunctionValue (JSONClearKey "{F}" "obj" 1)', False),
        ("after.clear.cache", f'SetFunctionValue (JSONToString "{F}")', True),
        ("set.nested.cache", f'SetFunctionValue (JSONSetValueFloat "{F}" "/obj/deep" 6 1)', False),
        ("after.nested.cache", f'SetFunctionValue (JSONToString "{F}")', True),
        ("uncached.set", 'SetFunctionValue (JSONSetValueFloat "nvfprobe3.json" "num" 3 0)', False),
        ("uncached.set.cache", 'SetFunctionValue (JSONSetValueFloat "nvfprobe3.json" "num" 4 1)', False),
        ("uncached.cached", 'SetFunctionValue (JSONIsFileCached "nvfprobe3.json")', False),
    ]
    risky = [
        ("erasekey.file", f'SetFunctionValue (JSONEraseKey "{F}" "str" 0)', False),
        ("erasekey.cache", f'SetFunctionValue (JSONEraseKey "{F}" "str" 1)', False),
        ("fromstring", 'SetFunctionValue (JSONCacheFileFromString "{\\"a\\":1}" "nvfmem.json")', False),
        ("fromstring.swap", 'SetFunctionValue (JSONCacheFileFromString "nvfmem2.json" "{\\"b\\":2}")', False),
        ("close.0", f'SetFunctionValue (JSONCloseFile "{F}" 0)', False),
        ("close.1", f'SetFunctionValue (JSONCloseFile "{F}" 1)', False),
    ]

    def job(name, rs):
        pre = [] if name == "json2" else [("cache", f'SetFunctionValue (JSONCacheFile "{F}" 0)', False)]
        after = [("after", f'SetFunctionValue (JSONToString "{F}")', True),
                 ("cached", f'SetFunctionValue (JSONIsFileCached "{F}")', False)]
        return _rows(f"json2.{name}", H, pre) + _rows("json2", H, rs) + _rows(f"json2.{name}", H, after)

    out = {"json2": job("json2", rows)}
    for i, r in enumerate(risky):
        out[f"json2r{i}"] = job(f"json2r{i}", [r])
    return out


_KH = ("ref rP\nref rA\nref rObj\nfloat fA\nfloat fB\nstring_var s\nstring_var k\narray_var aA\nBegin Function { }\n"
       'let rP := GetFormFromMod "FalloutNV.esm" "14"\nlet rA := rP.AuxVarGetRef "*nvfKN"\n')
# Doc Mitchell (0x104C0C) placed beside the player, then ten empty frames for him to settle.
_KPLACE = [("place", 'let rObj := GetFormFromMod "FalloutNV.esm" "104C0C"\nlet rA := rP.PlaceAtMe rObj 1\n'
            'rP.AuxVarSetRef "*nvfKN" rA\nSetFunctionValue 1', False)] + [(f"settle{i}", "SetFunctionValue 1", False) for i in range(10)]


def _knv1():
    # The paths written with doubled backslashes (a script string has no escapes, so the
    # game saw two); knv2 writes them single.
    IDLE = "characters\\\\_male\\\\locomotion\\\\mtidle.kf"
    FWD = "characters\\\\_male\\\\locomotion\\\\mtforward.kf"
    NOPE = "characters\\\\_male\\\\nvfnosuch.kf"
    rows = _KPLACE + [
        ("thisanim.player", "SetFunctionValue (rP.ThisAnim)", True),
        ("thisanim.num", "let fA := rP.ThisAnim\nSetFunctionValue fA", False),
        ("thisanimdir", "SetFunctionValue (rP.ThisAnimDir)", True),
        ("thisanim.noref", "SetFunctionValue (ThisAnim)", True),
        ("ejectweapon.doc.ret", "SetFunctionValue 0", False),
        ("pathbytype.0", "SetFunctionValue (rA.GetAnimPathBySequenceType 0)", True),
        ("pathbytype.0.1", "SetFunctionValue (rA.GetAnimPathBySequenceType 0 1)", True),
        ("pathbytype.player.0", "SetFunctionValue (rP.GetAnimPathBySequenceType 0)", True),
        ("pathbytype.player.1", "SetFunctionValue (rP.GetAnimPathBySequenceType 1)", True),
        ("pathbytype.player.3", "SetFunctionValue (rP.GetAnimPathBySequenceType 3)", True),
        ("pathbytype.form", "SetFunctionValue (GetAnimPathBySequenceType rA 0)", True),
    ] + [(f"{f}.{tag}", f'SetFunctionValue (rA.{f} "{p}")', t) for f, t in
         (("GetAnimWeight", False), ("GetAnimHandType", False), ("IsAnimAdditive", False), ("IsAnimQueued", False))
         for tag, p in (("idle", IDLE), ("fwd", FWD), ("nope", NOPE))] + [
        ("weight.player.idle", f'SetFunctionValue (rP.GetAnimWeight "{IDLE}")', False),
        ("handtype.noref", f'SetFunctionValue (GetAnimHandType "{IDLE}")', False),
        ("prio.idle", f'SetFunctionValue (rA.GetAnimInterpPriority "{IDLE}" "Bip01 Head")', False),
        ("prio.idle.root", f'SetFunctionValue (rA.GetAnimInterpPriority "{IDLE}" "Bip01")', False),
        ("prio.idle.none", f'SetFunctionValue (rA.GetAnimInterpPriority "{IDLE}" "NVFNone")', False),
        ("textkey.idle", f'SetFunctionValue (rA.GetAnimTextKeyTime "{IDLE}" "Start")', False),
        ("textkey.idle.end", f'SetFunctionValue (rA.GetAnimTextKeyTime "{IDLE}" "End")', False),
        ("textkey.idle.none", f'SetFunctionValue (rA.GetAnimTextKeyTime "{IDLE}" "NVFNone")', False),
        ("textkey.noref", f'SetFunctionValue (GetAnimTextKeyTime "{IDLE}" "Start")', False),
        ("dir.meshes", 'let aA := GetDirectoryFiles "Data\\\\Meshes"\nSetFunctionValue (sv_Construct "%g %z" (ar_Size aA) (TypeOf aA))', True),
        ("dir.nvse", 'let aA := GetDirectoryFiles "Data\\\\NVSE"\nSetFunctionValue (sv_Construct "%g %z" (ar_Size aA) (TypeOf aA))', True),
        ("dir.nvse.list", 'let aA := GetDirectoryFiles "Data\\\\NVSE"\nSetFunctionValue (sv_Join aA "|")', True),
        ("dir.nvse.1", 'let aA := GetDirectoryFiles "Data\\\\NVSE" 1\nSetFunctionValue (sv_Join aA "|")', True),
        ("dir.plugins", 'let aA := GetDirectoryFiles "Data\\\\NVSE\\\\Plugins"\nSetFunctionValue (sv_Join aA "|")', True),
        ("dir.root", 'let aA := GetDirectoryFiles ""\nSetFunctionValue (ar_Size aA)', False),
        ("dir.none", 'let aA := GetDirectoryFiles "NVFNoFolder"\nSetFunctionValue (sv_Construct "%g %z" (ar_Size aA) (TypeOf aA))', True),
        # Setters and the queue on Doc: their answers, then the getters again.
        ("setprio", f'SetFunctionValue (rA.SetAnimInterpPriority "{IDLE}" "Bip01 Head" 70)', False),
        ("prio.after", f'SetFunctionValue (rA.GetAnimInterpPriority "{IDLE}" "Bip01 Head")', False),
        ("copyprio", f'SetFunctionValue (rA.CopyAnimPriorities "{IDLE}" "{FWD}")', False),
        ("cycletype", f'SetFunctionValue (rA.SetAnimCycleType "{IDLE}" 0)', False),
        ("destframe", f'SetFunctionValue (rA.SetAnimDestFrame "{IDLE}" 5)', False),
        ("removetextkey", f'SetFunctionValue (rA.RemoveAnimTextKey "{IDLE}" 0)', False),
        ("additivemult", "SetFunctionValue (rA.SetAnimAdditiveInterpsWeightMult 0 0 0 0)", False),
        ("queue", f'SetFunctionValue (rA.QueueNextAnim "{FWD}")', False),
        ("queued.after", f'SetFunctionValue (rA.IsAnimQueued "{FWD}")', False),
        ("register", f'SetFunctionValue (RegisterCustomAnimGroup 0 "NVFProbeGroup")', False),
        ("createidle", f'let rObj := CreateIdleAnimForm "{IDLE}"\nSetFunctionValue (GetFormIDString rObj)', True),
        ("createidle.type", f'let rObj := CreateIdleAnimForm "{IDLE}"\nSetFunctionValue (GetType rObj)', False),
        ("createidle.nope", f'let rObj := CreateIdleAnimForm "{NOPE}"\nSetFunctionValue (GetFormIDString rObj)', True),
        ("erase", "SetFunctionValue (rA.EraseCurrentAnimAction)", False),
    ]
    return {"knv1": _rows("knv1", _KH, rows)}


def _knv2():
    IDLE = r"characters\_male\locomotion\mtidle.kf"
    FWD = r"characters\_male\locomotion\mtforward.kf"
    NOPE = r"characters\_male\nvfnosuch.kf"
    rows = _KPLACE + [
        ("pathbytype.0", "SetFunctionValue (rA.GetAnimPathBySequenceType 0)", True),
        ("pathbytype.2", "SetFunctionValue (rA.GetAnimPathBySequenceType 2)", True),
        ("pathbytype.player.0", "SetFunctionValue (rP.GetAnimPathBySequenceType 0)", True),
    ] + [(f"{f}.{tag}", f'SetFunctionValue (rA.{f} "{p}")', False) for f in
         ("GetAnimWeight", "GetAnimHandType", "IsAnimAdditive", "IsAnimQueued")
         for tag, p in (("idle", IDLE), ("fwd", FWD), ("nope", NOPE))] + [
        ("weight.player.idle", 'let s := rP.GetAnimPathBySequenceType 0\nSetFunctionValue (rP.GetAnimWeight $s)', False),
        ("prio.head", f'SetFunctionValue (rA.GetAnimInterpPriority "{IDLE}" "Bip01 Head")', False),
        ("prio.root", f'SetFunctionValue (rA.GetAnimInterpPriority "{IDLE}" "Bip01")', False),
        ("prio.none", f'SetFunctionValue (rA.GetAnimInterpPriority "{IDLE}" "NVFNone")', False),
        ("textkey.start", f'SetFunctionValue (rA.GetAnimTextKeyTime "{IDLE}" "Start")', False),
        ("textkey.end", f'SetFunctionValue (rA.GetAnimTextKeyTime "{IDLE}" "End")', False),
        ("textkey.none", f'SetFunctionValue (rA.GetAnimTextKeyTime "{IDLE}" "NVFNone")', False),
        ("dir.nvse", r'let aA := GetDirectoryFiles "Data\NVSE"' + '\nSetFunctionValue (sv_Join aA "|")', True),
        ("dir.nvse.1", r'let aA := GetDirectoryFiles "Data\NVSE" 1' + '\nSetFunctionValue (sv_Join aA "|")', True),
        ("dir.plugins", r'let aA := GetDirectoryFiles "Data\NVSE\Plugins"' + '\nSetFunctionValue (sv_Join aA "|")', True),
        ("dir.plugins.1", r'let aA := GetDirectoryFiles "Data\NVSE\Plugins" 1' + '\nSetFunctionValue (sv_Join aA "|")', True),
        ("dir.slash", r'let aA := GetDirectoryFiles "Data\NVSE\"' + '\nSetFunctionValue (sv_Join aA "|")', True),
        ("dir.none", 'let aA := GetDirectoryFiles "NVFNoFolder"\nSetFunctionValue (sv_Construct "%g %z" (ar_Size aA) (TypeOf aA))', True),
        ("setprio", f'SetFunctionValue (rA.SetAnimInterpPriority "{IDLE}" "Bip01 Head" 70)', False),
        ("prio.after", f'SetFunctionValue (rA.GetAnimInterpPriority "{IDLE}" "Bip01 Head")', False),
        ("copyprio", f'SetFunctionValue (rA.CopyAnimPriorities "{IDLE}" "{FWD}")', False),
        ("cycletype", f'SetFunctionValue (rA.SetAnimCycleType "{IDLE}" 0)', False),
        ("destframe", f'SetFunctionValue (rA.SetAnimDestFrame "{IDLE}" 5)', False),
        ("removetextkey", f'SetFunctionValue (rA.RemoveAnimTextKey "{IDLE}" 0)', False),
        ("queue", f'SetFunctionValue (rA.QueueNextAnim "{FWD}")', False),
        ("queued.after", f'SetFunctionValue (rA.IsAnimQueued "{FWD}")', False),
        ("register", 'RegisterCustomAnimGroup 0 "NVFProbeGroup"\nSetFunctionValue 1', False),
        ("ejectweapon.player", "SetFunctionValue (rP.EjectWeapon)", False),
    ]
    risky = [
        ("createidle", f'let rObj := CreateIdleAnimForm "{IDLE}"\nSetFunctionValue (sv_Construct "%z %g" (GetFormIDString rObj) (GetType rObj))', True),
        ("createidle.nope", f'let rObj := CreateIdleAnimForm "{NOPE}"\nSetFunctionValue (GetFormIDString rObj)', True),
        ("erase.player", "SetFunctionValue (rP.EraseCurrentAnimAction)", False),
    ]
    out = {"knv2": _rows("knv2", _KH, rows)}
    for i, r in enumerate(risky):
        out[f"knv2r{i}"] = _rows("knv2", _KH, [r])
    return out


def _knv3():
    H = ("ref rP\nref rA\nref rObj\nfloat fA\nfloat fB\nstring_var s\narray_var aA\nBegin Function { }\n"
         'let rP := GetFormFromMod "FalloutNV.esm" "14"\nlet rA := rP.AuxVarGetRef "*nvfKN"\n')
    P = 'let s := rA.GetAnimPathBySequenceType 0\n'
    rows = _KPLACE + [
        ("dir.star", r'let aA := GetDirectoryFiles "Data\NVSE\Plugins\*"' + '\nSetFunctionValue (sv_Construct "%g" (ar_Size aA))', False),
        ("dir.star.first", r'let aA := GetDirectoryFiles "Data\NVSE\Plugins\*"' + '\nSetFunctionValue aA[0]', True),
        ("dir.dll", r'let aA := GetDirectoryFiles "Data\NVSE\Plugins\*.dll"' + '\nSetFunctionValue (sv_Construct "%g" (ar_Size aA))', False),
        ("dir.dll.first", r'let aA := GetDirectoryFiles "Data\NVSE\Plugins\*.dll"' + '\nSetFunctionValue aA[0]', True),
        ("dir.dll.1", r'let aA := GetDirectoryFiles "Data\NVSE\Plugins\*.dll" 1' + '\nSetFunctionValue aA[0]', True),
        ("dir.folders", r'let aA := GetDirectoryFiles "Data\NVSE\*"' + '\nSetFunctionValue (sv_Join aA "|")', True),
        ("dir.folders.1", r'let aA := GetDirectoryFiles "Data\NVSE\*" 1' + '\nSetFunctionValue (sv_Join aA "|")', True),
        ("path", P + "SetFunctionValue s", True),
        ("weight", P + "SetFunctionValue (rA.GetAnimWeight $s)", False),
        ("weight.var", P + "let fA := rA.GetAnimWeight s\nSetFunctionValue fA", False),
        ("hand", P + "let fA := rA.GetAnimHandType s\nSetFunctionValue fA", False),
        ("additive", P + "let fA := rA.IsAnimAdditive s\nSetFunctionValue fA", False),
        ("prio", P + 'let fA := rA.GetAnimInterpPriority s "Bip01 Head"\nSetFunctionValue fA', False),
        ("textkey", P + 'let fA := rA.GetAnimTextKeyTime s "Start"\nSetFunctionValue fA', False),
        ("playgroupalt.idle", 'rA.PlayGroupAlt Idle 1\nSetFunctionValue 1', False),
        ("playgroupalt.fwd", 'rA.PlayGroupAlt Forward 0\nSetFunctionValue 1', False),
    ]
    return {"knv3": _rows("knv3", H, rows)}


def _small_helpers(forms):
    def fm(var, key):
        return f'let {var} := GetFormFromMod "FalloutNV.esm" "{forms[key]}"\n'

    def v(expr, pre=""):
        return pre + f"SetFunctionValue ({expr})"

    def ar(expr, pre=""):
        return pre + f'let aA := {expr}\nSetFunctionValue (sv_Construct "%g %z" (ar_Size aA) (TypeOf aA))'

    def idv(expr, pre=""):
        return pre + f"let rG := {expr}\nSetFunctionValue (GetFormIDString rG)"
    return fm, v, ar, idv


def _rest4():
    F = {"player": "14", "doc": "104C0C", "pistol": "434F", "caucasian": "19", "casino": "144C42", "music": "171B4A",
         "cams": "175C3A", "water": "178882", "perk": "31DE0", "quest": "80664", "greeting": "C8", "aloc": "171B4C",
         "ncr": "A46E7", "goodsprings": "104C6E", "stimpak": "15169", "info": "A471E"}
    H = ("ref rP\nref rA\nref rF\nref rG\nfloat fA\nfloat fB\nfloat fC\nfloat fD\nstring_var s\narray_var aA\nBegin Function { }\n"
         'let rP := GetFormFromMod "FalloutNV.esm" "14"\nlet rA := rP.AuxVarGetRef "*nvfR4"\n')
    fm, v, ar, idv = _small_helpers(F)
    rows = [
        ("place", fm("rF", "doc") + 'let rA := rP.PlaceAtMe rF 1\nrP.AuxVarSetRef "*nvfR4" rA\nSetFunctionValue 1', False),
    ] + [(f"settle{i}", "SetFunctionValue 1", False) for i in range(10)] + [
        # ITR
        ("cover.state.doc", v("rA.GetActorCoverState"), False),
        ("cover.info.doc", ar("rA.GetActorCoverInfo"), True),
        ("currentdialogue", ar("GetCurrentDialogue"), True),
        ("currentdialogue.player", ar("rP.GetCurrentDialogue"), True),
        ("eligibleperks", ar("rP.GetEligiblePerksEx 0"), True),
        ("eligibleperks.1", ar("rP.GetEligiblePerksEx 1"), True),
        # JohnnyGuitar
        ("camshotpath", v("GetCameraShotPath rF", fm("rF", "cams")), True),
        ("casinodeck", ar("GetCasinoDeckTexture rF", fm("rF", "casino")), True),
        ("casinodeck.0", v("GetCasinoDeckTexture rF 0", fm("rF", "casino")), True),
        ("accmult.player", v("rP.GetExtraAccuracyPenaltyMult"), False),
        ("accmult.str", v('rP.GetExtraAccuracyPenaltyMult "x"'), False),
        ("accmult.set", v('rP.SetExtraAccuracyPenaltyMult 2 "nvf"'), False),
        ("accmult.after", v('rP.GetExtraAccuracyPenaltyMult "nvf"'), False),
        ("accmult.after.all", v("rP.GetExtraAccuracyPenaltyMult"), False),
        ("accmult.remove", v('rP.RemoveExtraAccuracyPenaltyMult "nvf"'), False),
        ("accmult.removed", v('rP.GetExtraAccuracyPenaltyMult "nvf"'), False),
        ("switchnode", v('rA.GetSwitchNodeIndex "Bip01 Head"'), False),
        ("musictypepath.set", v('SetMusicTypePath rF "nvf\\x.mp3"', fm("rF", "music")), False),
        ("camshotpath.set", v('SetCameraShotPath rF "nvf\\x.nif"', fm("rF", "cams")), False),
        ("camshotpath.after", v("GetCameraShotPath rF", fm("rF", "cams")), True),
        ("mlc.set", v("SetMediaLocationControllerOverride rF", fm("rF", "aloc")), False),
        ("mlc.clear", v("ClearMediaLocationControllerOverride"), False),
        # One111
        ("weapfloat.0", v("GetWeapAdditionalFloat rF 0", fm("rF", "pistol")), False),
        ("weapfloat.1", v("GetWeapAdditionalFloat rF 1", fm("rF", "pistol")), False),
        ("weapfloat.set", v("SetWeapAdditionalFloat rF 0 2.5", fm("rF", "pistol")), False),
        ("weapfloat.after", v("GetWeapAdditionalFloat rF 0", fm("rF", "pistol")), False),
        ("weapflag.set", v("SetWeapAdditionalFlag rF 0 1", fm("rF", "pistol")), False),
        ("damagecalcflag", v("SetOnDamageCalcFlag 0"), False),
        # ShowOff
        ("isTempRef.player", v("rP.IsTemporaryReference"), False),
        ("isTempRef.doc", v("rA.IsTemporaryReference"), False),
        ("isTempRef.docref", v("rF.IsTemporaryReference", 'let rF := GetFormFromMod "FalloutNV.esm" "104C0F"\n'), False),
        ("worldoffset", ar("rA.GetWorldOffsetPosArray"), True),
        ("worldoffset.vals", 'let aA := rA.GetWorldOffsetPosArray\nlet fA := aA[0]\nlet fB := aA[1]\nlet fC := aA[2]\n'
         'SetFunctionValue (sv_Construct "%.1f %.1f %.1f" fA fB fC)', True),
        ("levelup.unspent.0", v("GetLevelUpMenuUnspentPoints 0"), False),
        ("levelup.unspent.1", v("GetLevelUpMenuUnspentPoints 1"), False),
        ("pipboyradio", ar("GetPipboyRadioVoiceEntryData 0"), True),
        ("noequip.get", v("GetNoEquipShowOff rF 0", fm("rF", "stimpak")), False),
        ("noequip.set", v("SetNoEquipShowOff rF 1", fm("rF", "stimpak")), False),
        ("noequip.after", v("GetNoEquipShowOff rF 0", fm("rF", "stimpak")), False),
        ("noequip.unset", v("SetNoEquipShowOff rF 0", fm("rF", "stimpak")), False),
        ("sayto.actor", idv("SayTo_GetSpeakingActor"), True),
        ("sayto.quest", idv("SayTo_GetQuest"), True),
        ("liveeffect.trait", v("GetLiveEffectBaseTrait 0"), False),
        ("showingoff.disable", v("ShowingOffDisable"), False),
        ("showingoff.enable", v("ShowingOffEnable"), False),
        # SUP
        ("nifids.name", ar('rA.GetNifChildrenIDsByName "Bip01"'), True),
        ("nifids.name.head", ar('rA.GetNifChildrenIDsByName "Bip01 Head"'), True),
        ("nifids.id", ar("rA.GetNifChildrenIDsByID 0 0"), True),
        ("niftype.0", v("rA.GetNifTypeByID 0 0"), True),
        ("niftype.1", v("rA.GetNifTypeByID 1 0"), True),
        ("nifstring.0", v("rA.GetNifStringByID 0 0"), True),
        ("nifstring.1", v("rA.GetNifStringByID 1 0"), True),
        ("comparenif", ar('rA.CompareNifBlocks "Bip01 Head" 0'), True),
        ("condvar", v("SUPGetConditionVarFloat 0"), False),
        ("topicquest", idv("TopicGetTopicInfoQuest rF", fm("rF", "info")), True),
        ("topicquest.dial", idv("TopicGetTopicInfoQuest rF", fm("rF", "greeting")), True),
        ("hudbar.hide", v('HudBarSetForceHide 0 0 "a"'), False),
    ]
    risky = [
        ("limbvel", v("rA.SetDismemberedLimbVelocity 0 0 0 0"), False),
        ("tempparticle", v("SpawnTempParticle 0 0 0 0 0 0 0 0 0"), False),
        ("levelup.set", v("SetLevelUpMenuPoints 0 0"), False),
        ("liveeffect.set", v("SetLiveEffectMagnitudeModifier 0 0"), False),
        ("facegen.copy", v("BaseCopyFaceGenFrom rF rF", fm("rF", "doc")), False),
        ("messagealt", v('MessageExAltShowoff 0 0 "nvf probe"'), False),
    ]
    out = {"rest4": _rows("rest4", H, rows)}
    for i, r in enumerate(risky):
        out[f"rest4r{i}"] = _rows(f"rest4.rest4r{i}", H, [rows[0]]) + _rows("rest4", H, [r])
    return out


def _rest5():
    F = {"doc": "104C0C", "pistol": "434F", "caucasian": "19", "caucold": "42C3", "script": "107230", "bptd": "1690AD",
         "quest": "80664", "water": "178882", "crate": "179B2C", "opensnd": "76E93", "closesnd": "76E94", "voice": "173561",
         "rock": "144BA9", "rockit": "4BDF0"}
    H = ("ref rP\nref rA\nref rF\nref rG\nfloat fA\nfloat fB\nstring_var s\narray_var aA\nBegin Function { }\n"
         'let rP := GetFormFromMod "FalloutNV.esm" "14"\nlet rA := rP.AuxVarGetRef "*nvfR5"\n')
    fm, v, ar, idv = _small_helpers(F)
    rows = [
        ("place", fm("rF", "doc") + 'let rA := rP.PlaceAtMe rF 1\nrP.AuxVarSetRef "*nvfR5" rA\nSetFunctionValue 1', False),
    ] + [(f"settle{i}", "SetFunctionValue 1", False) for i in range(10)] + [
        ("raceagerace.0", idv("GetRaceAgeRace rF 0", fm("rF", "caucasian")), True),
        ("raceagerace.1", idv("GetRaceAgeRace rF 1", fm("rF", "caucasian")), True),
        ("raceagerace.old.0", idv("GetRaceAgeRace rF 0", fm("rF", "caucold")), True),
        ("racevoice.0", idv("GetRaceVoice rF 0", fm("rF", "caucasian")), True),
        ("racevoice.1", idv("GetRaceVoice rF 1", fm("rF", "caucasian")), True),
        ("scriptblock.0", v("GetScriptBlockDisabled rF 0", fm("rF", "script")), False),
        ("scriptblock.7", v("GetScriptBlockDisabled rF 7", fm("rF", "script")), False),
        ("bodypartvats.0", v("GetBodyPartVATSTarget rF 0", fm("rF", "bptd")), True),
        ("bodypartvats.1", v("GetBodyPartVATSTarget rF 1", fm("rF", "bptd")), True),
        ("objtele", ar("GetObjectiveTeleportLinks rF 10", fm("rF", "quest")), True),
        ("shotspersec.pistol", v("CalculateShotsPerSec rF", fm("rF", "pistol")), False),
        ("shotspersec.player", v("CalculateShotsPerSec rP"), False),
        ("equippedarmor.player", ar("rP.GetEquippedArmorRefs"), True),
        ("equippedarmor.doc", ar("rA.GetEquippedArmorRefs"), True),
        ("collisionnodes.doc", ar("rA.GetCollisionNodes"), True),
        ("hasactionref.doc", v("rA.HasActionRef"), False),
        ("attackqueued.doc", v("rA.IsAttackQueued"), False),
        ("questtargetschanged", v("GetQuestTargetsChanged"), False),
        ("recipesubcats", v("GetRecipeMenuNumSubcategories"), False),
        ("worldmapmults", "SetFunctionValue 0", False),
        ("refmap.set", v('RefMapArraySetValue "*nvfrm" 5 rP'), False),
        ("refmap.get", v('RefMapArrayGetValue "*nvfrm" rP'), False),
        ("refmap.getsize", v('RefMapArrayGetSize "*nvfrm"'), False),
        ("refmap.getf", v('RefMapArrayGetFloat "*nvfrm" rP'), False),
        ("refmap.set.str", v('RefMapArraySetValue "*nvfrm" "txt" rA'), False),
        ("refmap.get.str", v('RefMapArrayGetValue "*nvfrm" rA'), True),
        ("arrayvalue", 'let aA := ar_List 10 20 30\nSetFunctionValue (GetArrayValue aA 1)', False),
        ("arrayvalue.ref", v("GetArrayValue rP 0"), False),
        ("opensound.set", fm("rF", "crate") + fm("rG", "closesnd") + "let fA := SetContainerOpenSound rG rF\n"
         "let rG := GetContainerOpenSound rF\nSetFunctionValue (sv_Construct \"%g %z\" fA (GetFormIDString rG))", True),
        ("opensound.restore", fm("rF", "crate") + fm("rG", "opensnd") + "SetFunctionValue (SetContainerOpenSound rG rF)", False),
        ("closesound.set", fm("rF", "crate") + fm("rG", "opensnd") + "let fA := SetContainerCloseSound rG rF\n"
         "let rG := GetContainerCloseSound rF\nSetFunctionValue (sv_Construct \"%g %z\" fA (GetFormIDString rG))", True),
        ("closesound.restore", fm("rF", "crate") + fm("rG", "closesnd") + "SetFunctionValue (SetContainerCloseSound rG rF)", False),
        ("racevoice.set", fm("rF", "caucasian") + "let rG := GetRaceVoice rF 0\nlet fA := SetRaceVoice rF 0 rG\nSetFunctionValue fA", False),
        ("raceagerace.set", fm("rF", "caucasian") + "let rG := GetRaceAgeRace rF 0\nlet fA := SetRaceAgeRace rF 0 rG\nSetFunctionValue fA", False),
        ("scriptblock.set", v("SetScriptBlockDisabled rF 7 0", fm("rF", "script")), False),
        ("watertrait", v("SetWaterTrait rF 0 0", fm("rF", "water")), False),
        ("toggle.depth", v("ToggleDepthClear 0"), False),
        ("toggle.hiteffects", v("rA.ToggleHitEffects 1"), False),
        ("toggle.nomovement", v("rA.ToggleNoMovementCombat 0"), False),
        ("toggle.nozpos", v("rA.ToggleNoZPosReset 0"), False),
        ("toggle.nodecoll", v('rA.ToggleNodeCollision "Bip01 Head" 1'), False),
        ("suppressquest", v("SuppressQuestMessages"), False),
        ("unlockrecipe", v("UnlockRecipeMenuQuantity 0"), False),
        ("setitemfilter", v('SetItemFilter "nvf"'), False),
        ("textransform", v("rA.SetTextureTransformKey 0 0 0 0"), False),
        ("clearalttex", v("ClearModelAltTextures rF", fm("rF", "pistol")), False),
        ("rockit", v("InitRockItLauncher rF", fm("rF", "pistol")), False),
        ("improvedrecipe", v("EnableImprovedRecipeMenu"), False),
    ]
    risky = [
        ("windspeed", v("SetWindSpeedMult 1")),
        ("swaplod", v("SwapObjectLOD rF 0 0", fm("rF", "rock"))),
        ("scrollwheel", v("ScrollMouseWheel 0")),
        ("initfilter", v("InitItemFilter")),
        ("initminimap", v("InitMiniMap")),
        ("updateminimap", v("UpdateMiniMap 0")),
        ("projecttile", v("ProjectUITile 0 0 0 0 0 0")),
        ("reloadchar", v("rA.ReloadCharController")),
        ("reloadclouds", v("ReloadCloudTextures")),
        ("resetclouds", v("ResetClouds")),
        ("executescript", v("ExecuteScript")),
        ("levelupex", v("ShowLevelUpMenuEx 0")),
    ]
    out = {"rest5": _rows("rest5", H, rows)}
    for i, (k, b) in enumerate(risky):
        out[f"rest5r{i:02d}"] = _rows(f"rest5.rest5r{i:02d}", H, [rows[0]]) + _rows("rest5", H, [(k, b, False)])
    return out


SETS = {}
for _build in (_json1, _json2, _knv1, _knv2, _knv3, _rest4, _rest5):
    SETS.update(_build())

# Every set here changes the game (places Doc Mitchell, sets values) or a file (json*).
WRITING = set(SETS)

# The launches the original game did not finish (see the module's docstring): their
# goldens hold the rows before the stop, and the lab has no scenario for those with none.
STOPPED = {"json1": 37, "knv1": 60, "json2r0": 1, "json2r1": 1, "knv2r0": 0, "knv2r1": 0}


# ---- The compile-only argument-shape run (docs/spec-notes/ext-unlisted-rest.md §1) ----

# The 205 names nvse.log registers that had no catalogue row on 2026-11-13, less the two
# container sounds #2300 had built (#2318's 33 among them, which this run also checked).
SHAPE_NAMES = [
    "AddPerkEntry", "AuxStringMapArrayDestroy", "AuxStringMapArrayEraseKey",
    "AuxStringMapArrayGetAll", "AuxStringMapArrayGetAsArray", "AuxStringMapArrayGetFirst",
    "AuxStringMapArrayGetFloat", "AuxStringMapArrayGetKeys", "AuxStringMapArrayGetNext",
    "AuxStringMapArrayGetRef", "AuxStringMapArrayGetSize", "AuxStringMapArrayGetString",
    "AuxStringMapArrayGetValueType", "AuxStringMapArraySetFloat",
    "AuxStringMapArraySetFromArray", "AuxStringMapArraySetRef", "AuxStringMapArraySetString",
    "AuxStringMapArrayValidateValues", "BBAPIKeyExist", "BBAreMessagesFetched",
    "BBDeleteUserMessage", "BBGetBBKey", "BBGetCurrentNotesInCell", "BBGetInfoForReference",
    "BBGetTextMenuString", "BBGetUserInfo", "BBGetUserMessages", "BBHideMessage",
    "BBIsBBLoaded", "BBIsReferenceNote", "BBIsTextEditMenuActive", "BBLikeMessage",
    "BBNexusAPIKeyExist", "BBOnCellChange", "BBPostMessage", "BBReportMessage",
    "BBReturnAllMessagesToDefaultCell", "BBSetTextMenuTitle", "BaseCopyFaceGenFrom",
    "CCCTaskPackageFlags", "CalculateShotsPerSec", "ClearMediaLocationControllerOverride",
    "ClearModelAltTextures", "CompareNifBlocks", "CopyAnimPriorities", "CreateIdleAnimForm",
    "DumpTileInfoChilds", "ERDTestFunc", "EjectWeapon", "EnableImprovedRecipeMenu",
    "EraseCurrentAnimAction", "ExecuteScript", "GetActorCoverInfo", "GetActorCoverState",
    "GetAnimHandType", "GetAnimInterpPriority", "GetAnimPathBySequenceType",
    "GetAnimTextKeyTime", "GetAnimWeight", "GetArrayValue", "GetBodyPartVATSTarget",
    "GetCalculatedPosFrame", "GetCameraMovement", "GetCameraShotPath", "GetCasinoDeckTexture",
    "GetCollisionNodes", "GetCurrentDialogue", "GetDirectoryFiles", "GetEligiblePerksEx",
    "GetEquippedArmorRefs", "GetExtraAccuracyPenaltyMult", "GetFuncCodeByName",
    "GetFuncNameByCode", "GetLevelUpMenuUnspentPoints", "GetLiveEffectBaseTrait",
    "GetNifBlockTranslationToVars", "GetNifChildrenIDsByID", "GetNifChildrenIDsByName",
    "GetNifStringByID", "GetNifTypeByID", "GetNoEquipShowOff", "GetObjectiveTeleportLinks",
    "GetPackedPlayerFOV", "GetPastebinReaderCurrentStage", "GetPastebinReaderDoneStage",
    "GetPipboyRadioVoiceEntryData", "GetQuestTargetsChanged", "GetRaceAgeRace", "GetRaceVoice",
    "GetRecipeMenuNumSubcategories", "GetSUPDateCheckerCurrentStage",
    "GetSUPDateCheckerDoneStage", "GetScriptBlockDisabled", "GetSwitchNodeIndex",
    "GetTranslatedPos", "GetWeapAdditionalFloat", "GetWorldMapPosMults",
    "GetWorldOffsetPosArray", "HasActionRef", "HudBarGetPos", "HudBarSetForceHide",
    "InitItemFilter", "InitMiniMap", "InitRockItLauncher", "IsAnimAdditive", "IsAnimQueued",
    "IsAttackQueued", "IsJumping_BROKEN", "IsNewGame", "IsTemporaryReference",
    "JGLegacyWorldToScreen", "JSONAppendValueFloat", "JSONAppendValueString", "JSONCacheFile",
    "JSONCacheFileFromString", "JSONClearKey", "JSONCloseFile", "JSONEraseArrayElement",
    "JSONEraseKey", "JSONGetValueArray", "JSONGetValueType", "JSONIsFileCached",
    "JSONSetValueFloat", "JSONSetValueString", "JSONToString", "ListAddArray_OLD", "LunetteCMD",
    "Matrix3x3_GetQuaternion_OLD", "Matrix_ApplyOperationWithMatrix_OLD",
    "Matrix_ApplyOperationWithScalar_OLD", "Matrix_Dump_OLD", "Matrix_IsMatrix_OLD",
    "Matrix_Transpose_OLD", "MessageExAltShowoff", "NullArgs", "PlayGroupAlt", "ProjectUITile",
    "Quaternion_GetMatrix_OLD", "QueueNextAnim", "RefAddrxData", "RefMapArrayGetValue",
    "RefMapArraySetValue", "RegisterCustomAnimGroup", "ReloadCharController",
    "ReloadCloudTextures", "RemoveAnimTextKey", "RemoveExtraAccuracyPenaltyMult", "ResetClouds",
    "SUPGetConditionVarFloat", "SUPTest", "SUPWRITETOLOG", "SayTo_GetQuest",
    "SayTo_GetSpeakingActor", "ScrollMouseWheel", "SetAnimAdditiveInterpsWeightMult",
    "SetAnimCycleType", "SetAnimDestFrame", "SetAnimInterpPriority", "SetBlockTransform",
    "SetCameraShotPath", "SetCasinoDeckTexture", "SetContainerCloseSound",
    "SetContainerOpenSound", "SetDialogResponseOverrideValues", "SetDismemberedLimbVelocity",
    "SetExtraAccuracyPenaltyMult", "SetFactionCombatReactionTemp", "SetItemFilter",
    "SetLevelUpMenuPoints", "SetLiveEffectMagnitudeModifier",
    "SetMediaLocationControllerOverride", "SetMusicTypePath", "SetNoEquipShowOff",
    "SetOnDamageCalcFlag", "SetRaceAgeRace", "SetRaceVoice", "SetScriptBlockDisabled",
    "SetTextureTransformKey", "SetWaterTrait", "SetWeapAdditionalFlag",
    "SetWeapAdditionalFloat", "SetWindSpeedMult", "ShowLevelUpMenuEx", "ShowingOffDisable",
    "ShowingOffEnable", "SpawnTempParticle", "StringContainsOnlyASCII", "SuppressQuestMessages",
    "SwapObjectLOD", "ThisAnim", "ThisAnimDir", "ToggleDepthClear", "ToggleHitEffects",
    "ToggleNoMovementCombat", "ToggleNoZPosReset", "ToggleNodeCollision",
    "TopicGetTopicInfoQuest", "TopicInfoSetResponseStrings_OLD", "TopicReplaceTopicInfo",
    "TransformWorldToLocal", "UnlockRecipeMenuQuantity", "UpdateCheckerCheckForUpdates",
    "UpdateCheckerReturnArray", "UpdateMiniMap", "kNVSEReset"
]
SHAPE_PATTERNS = ["", "0", "0 0", "0 0 0", "0 0 0 0", "0 0 0 0 0", "0 0 0 0 0 0", "0 0 0 0 0 0 0 0 0",
                  '"a"', '"a" 0', '"a" 0 0', '"a" "b"', '"a" "b" 0', "rP", "rP 0", "rP 0 0", "0 \"a\"", "0 0 \"a\"",
                  "rP \"a\"", "\"a\" rP", "rP rP", "X", "X 0"]


def shape_steps(names=None):
    """Every name with each pattern inside `if 0` (compiled, never run); value 1 = compiled,
    an error = refused. Labels are `<name>|<pattern index>`."""
    steps = []
    for n in names or SHAPE_NAMES:
        for i, p in enumerate(SHAPE_PATTERNS):
            body = f"ref rP\nBegin Function {{ }}\nif 0\n  {n} {p}\nendif\nSetFunctionValue 1\nEnd\n"
            steps.append({"query": body, "label": f"{n}|{i}", "field": "value"})
    return steps


def shape_jobs(out_dir, fmt, chunk=150):
    """Write the run as jobs shp00, shp01, ... of `chunk` steps; returns their paths."""
    steps = shape_steps()
    paths = []
    for k in range(0, len(steps), chunk):
        name = f"shp{k // chunk:02d}"
        path = os.path.join(out_dir, f"{name}.steps.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            json.dump({"format": fmt, "name": name, "title": "unlisted command shapes (compile only)",
                       "start": "any save (read-only)", "steps": steps[k:k + chunk]}, f, indent=0)
            f.write("\n")
        paths.append(path)
    return paths


def read_shapes(outputs):
    """name -> the patterns that compiled, from the shape jobs' capture outputs (.jsonl)."""
    ok = {}
    for out in outputs:
        with open(out, encoding="utf-8") as f:
            for line in f:
                r = json.loads(line)
                if r.get("kind") != "result":
                    continue
                n, i = r["label"].split("|")
                ok.setdefault(n, [])
                if "error" not in r:
                    ok[n].append(SHAPE_PATTERNS[int(i)] or "(none)")
    return ok
