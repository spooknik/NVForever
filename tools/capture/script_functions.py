#!/usr/bin/env python3
"""Build the script-function catalogue (`fnv.script.functions/1`) from three sources.

1. A private MediaWiki export of the GECK wiki category "Functions (All)"
   (geckwiki.com, Special:Export, current revisions only). Only the *structured* fields of
   each page's {{Function}} template are taken (name, alias, origin, calling reference,
   return value, argument names/types/optionality, categories). Page prose is never copied;
   human-written paraphrases live in `schemas/script/notes.json` and are merged in.
2. The owner's masters (read-only). Compiled script data (SCDA) is lined up against the
   script source (SCTX) shipped next to it, which yields the opcode of every function that
   the shipped scripts call, plus usage counts, Begin-block event codes and condition
   (CTDA) usage. See docs/spec-notes/script-functions.md section 3 for the observations.
3. Optionally xEdit's `Core/wbDefinitionsFNV.pas` (MPL-2.0, TES5Edit/TES5Edit, branch
   dev-4.1.6): its condition-function table gives an index per function name.

Outputs are our own JSON and Markdown; no wiki text and no game content (only names that
the wiki also documents, opcodes, and counts) is written.

usage:
  script_functions.py --wiki C:/fnv-private/geckwiki/functions.xml
                      --data "<FNV>/Data" [--xedit wbDefinitionsFNV.pas]
                      [--notes schemas/script/notes.json]
                      [--out schemas/script/functions.json]
                      [--out-md docs/spec-notes/script-functions-catalogue.md]

Standard library only. Exit codes: 0 ok, 1 input unreadable, 2 usage error.
"""
import argparse
import collections
import html
import json
import os
import re
import struct
import sys
import xml.etree.ElementTree as ET
import zlib

SCHEMA = "fnv.script.functions/1"
WIKI_URL = "https://geckwiki.com/index.php?title="
MASTERS = ["FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm",
           "LonesomeRoad.esm", "GunRunnersArsenal.esm", "CaravanPack.esm",
           "ClassicPack.esm", "MercenaryPack.esm", "TribalPack.esm"]

# Origin codes used by the wiki's {{Function}} template. Only the first three are part of
# the unmodified game; everything else needs a script extender or one of its plugins.
VANILLA_ORIGINS = {"GECK1": "base game (present since the Fallout 3 GECK)",
                   "VEGAS1": "base game (added in New Vegas)",
                   "CONSOLENV": "base game console command"}

# The core extender families (docs/handoff/extender-core-tasks.md, task N1): an NVSE or
# FOSE1 function in one of them gets `core_family`, the name of the spec note
# (docs/spec-notes/ext-<family>.md) that covers it.
CORE_ORIGINS = {"NVSE", "FOSE1"}
CORE_NAMED = {"values": {"typeof", "isformvalid", "isreference"},
              "strings": {"tostring", "tonumber", "numtohex", "getformidstring"},
              "udf": {"call", "setfunctionvalue", "getcallingscript"},
              "events": {"dispatchevent"}}


def core_family(rec):
    """The core family of an extender function, or None."""
    if rec["origin"] not in CORE_ORIGINS:
        return None
    n = rec["name"].lower()
    if n.startswith("sv_"):
        return "strings"
    if n.startswith("ar_"):
        return "arrays"
    if "eventhandler" in n:
        return "events"
    return next((fam for fam, names in CORE_NAMED.items() if n in names), None)


# JIP LN NVSE (docs/handoff/jip-tasks.md, task J1): a JIP or LU (Lutana, now part of
# JIP) function gets `jip_family`, its wiki category without the "(JIP)" suffix, and
# `jip_probe`, how a probe can call it: "pure" (numbers, strings or axes only, no calling
# reference: a runtime-compiled capture query can), "form" (it needs forms or a calling
# reference, built with BuildRef), "none" (variables, arrays or other arguments, or a
# setter returning nothing: a GECK-compiled probe).
JIP_ORIGINS = {"JIP", "LU"}
PURE_KINDS = {"int", "float", "string", "axis"}
FORM_KINDS = PURE_KINDS | {"reference", "form", "actor_value"}


def jip_family(rec):
    """The JIP family of a function (its first wiki category, "(JIP)" ones first), or None."""
    if rec["origin"] not in JIP_ORIGINS:
        return None
    cats = [c for c in rec.get("categories", []) if not c.startswith("Functions")]
    own = [c for c in cats if c.endswith(" (JIP)")]
    pick = (own or cats or [None])[0]
    return pick[:-len(" (JIP)")] if pick and pick.endswith(" (JIP)") else pick


def jip_probe(rec):
    """How a probe can call a JIP function: "pure", "form" or "none"."""
    kinds = {p.get("kind") for p in rec.get("params", [])}
    ret = ((rec.get("returns") or {}).get("type") or "").lower()
    if ret in ("void", "none", "nothing"):
        return "none"
    if kinds <= PURE_KINDS and not rec.get("calling_reference"):
        return "pure"
    if kinds <= FORM_KINDS:
        return "form"
    return "none"


# Statement opcodes below 0x1000 seen in shipped bytecode, named by lining up the source
# line [observed]. Function opcodes are 0x1000 and above.
STATEMENT_OPCODES = {0x10: "Begin", 0x11: "End", 0x15: "Set", 0x16: "If", 0x17: "Else",
                     0x18: "ElseIf", 0x19: "EndIf", 0x1C: "(reference prefix)",
                     0x1D: "ScriptName", 0x1E: "Return"}
EXPRESSION_STATEMENTS = (0x15, 0x16, 0x18, 0x1E)
DECLARATION = re.compile(r"^(short|int|long|float|ref|reference)\b", re.I)
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


# --------------------------------------------------------------------------- wiki dump

def _template_body(text, name):
    m = re.search(r"\{\{\s*" + name + r"\b", text)
    if not m:
        return None
    i = j = m.end()
    depth = 1
    while j < len(text) and depth:
        if text.startswith("{{", j):
            depth += 1
            j += 2
        elif text.startswith("}}", j):
            depth -= 1
            j += 2
        else:
            j += 1
    return text[i:j - 2]


def _split_top(body):
    """Split template parameters on '|' outside nested templates and links."""
    parts, cur, depth, link, i = [], [], 0, 0, 0
    while i < len(body):
        two = body[i:i + 2]
        if two in ("{{", "}}", "[[", "]]"):
            depth += {"{{": 1, "}}": -1}.get(two, 0)
            link += {"[[": 1, "]]": -1}.get(two, 0)
            cur.append(two)
            i += 2
            continue
        if body[i] == "|" and depth == 0 and link == 0:
            parts.append("".join(cur))
            cur = []
        else:
            cur.append(body[i])
        i += 1
    parts.append("".join(cur))
    return parts


def template_fields(body):
    body = re.sub(r"<nowiki>(.*?)</nowiki>", lambda m: m.group(1).replace("|", "\0"),
                  body, flags=re.S)
    out = {}
    for part in _split_top(body):
        if "=" in part:
            key, value = part.split("=", 1)
            out[key.strip()] = value.strip().replace("\0", "|")
    return out


def plain(s):
    s = re.sub(r"<ref.*?</ref>", "", s, flags=re.S)
    s = re.sub(r"\[\[(?:[^\]|]*\|)?([^\]]*)\]\]", r"\1", s)
    s = re.sub(r"'''?", "", s)
    s = re.sub(r"<[^>]+>", "", s)
    s = re.sub(r"\{\{[^}]*\}\}", "", s)
    return re.sub(r"\s+", " ", html.unescape(s)).strip()


def _truthy(v):
    return v.strip().lower() in ("y", "yes", "true", "1")


def param_type(raw):
    """The argument's type word(s) only. A page may follow the type with its own table
    (`int == HUD Flags == kActionPoints = 0x1,// BIT - 0 ...`): that is page text, not a
    type, and the catalogue does not take it over (extender-compat.md §4.1 item 2)."""
    if raw is None:
        return None
    t = re.split(r"==|//|=", raw, maxsplit=1)[0].strip()
    return t or None


def arg_kind(raw):
    """Map the wiki's free-form argument type to a small vocabulary (raw is kept too)."""
    t = (raw or "").lower().replace(" ", "")
    if not t:
        return "unknown"
    if t.startswith(("int", "integer", "uint", "short", "long", "bool", "0/1", "1/0",
                     "(0/1)", "0/1/2", "1/2", "bitmask", "char")):
        return "int"
    if t.startswith(("float", "double", "number")):
        return "float"
    if t in ("x/y/z", "x/y", "axis"):
        return "axis"
    if t in ("statname", "actorvalue", "actorvaluename"):
        return "actor_value"
    if t.startswith(("string", "editorid", "formattedstring", "formatable", "text")):
        return "string"
    if t.startswith(("array", "anyarray")):
        return "array"
    if t.startswith(("scriptvar", "var", "variable", "returnvariable")):
        return "variable"
    if t.startswith(("ref", "objectref", "actorref", "reference", "callingref")):
        return "reference"
    if t.startswith(("form", "baseform", "anyform", "object", "npc", "quest", "faction",
                     "formlist", "race", "cell", "worldspace", "global", "topic", "perk",
                     "weapon", "armor", "actor", "container", "script", "camerashot",
                     "actorbase", "regionid")):
        return "form"
    return "other"


def parse_wiki(path):
    ns = {"m": "http://www.mediawiki.org/xml/export-0.10/"}
    root = ET.parse(path).getroot()
    out = []
    for page in root.findall("m:page", ns):
        if page.find("m:ns", ns).text != "0":
            continue
        title = page.find("m:title", ns).text
        text = page.find("m:revision/m:text", ns).text or ""
        rec = parse_page(title, text)
        if rec:
            out.append(rec)
    return out


def _item_mod_handler(flag):
    """ppNVSE's OnAttachItemMod / OnDetachItemMod: priority, script, flag, then up to two
    filters, each a number ("1" the item mod, "2" the object) and a form."""
    opt = {"optional": True}
    return [{"name": "priority", "type": "int", "kind": "int"},
            {"name": "script", "type": "ref", "kind": "reference"},
            {"name": flag, "type": "bool", "kind": "int", **opt},
            {"name": "filterIndex1", "type": "string", "kind": "string", **opt},
            {"name": "filter1", "type": "form", "kind": "form", **opt},
            {"name": "filterIndex2", "type": "string", "kind": "string", **opt},
            {"name": "filter2", "type": "form", "kind": "form", **opt}]


# Parameters where a page's argument table is missing or short: what the game took
# [observed: the probe named] or what the page's own example passes [documented].
PARAM_FIXES = {
    # [documented: SetCursorTrait's example "SetCursorTrait 0 255"; the table lists the
    # trait only].
    "SetCursorTrait": [{"name": "iTrait", "type": "int", "kind": "int"},
                       {"name": "fValue", "type": "float", "kind": "float"}],
    # [observed: jip-probes rules1/rules2 (#1021): `let x := ref.SetNthPackage package
    # index` compiled in the game and replaced the package at the index].
    "SetNthPackage": [{"name": "package", "type": "form", "kind": "form"},
                      {"name": "index", "type": "int", "kind": "int"}],
    # [documented: GetCellEncounterZone's example "GetCellEncounterZone VikkiAndVance";
    # observed: showoff2, Doc Mitchell's house].
    "GetCellEncounterZone": [{"name": "cell", "type": "form", "kind": "form"}],
    # [documented: SetZoneMinLevel's example "SetZoneMinLevel YaoGauiDen01 10"; the table
    # lists the zone only].
    "SetZoneMinLevel": [{"name": "encounterZone", "type": "form", "kind": "form"},
                        {"name": "level", "type": "int", "kind": "int"}],
    # [documented: SetMusicTypeDB's example "SetMusicTypeDB DefaultSuccess -100"].
    "SetMusicTypeDB": [{"name": "musicType", "type": "ref", "kind": "reference"},
                       {"name": "dB", "type": "float", "kind": "float"}],
    # [documented: GetAttachedMod's examples, the weapon then the slot ("1",
    # "Barrel Device"); the table lists the weapon only].
    "GetAllAttachedWeaponMods": [{"name": "weapon", "type": "ref", "kind": "reference"},
                                 {"name": "slot", "type": "string", "kind": "string",
                                  "optional": True}],
    # [documented: OnAttachItemMod's example "OnAttachItemMod 1 OnAttachScript 1 "1"
    # itemMod "2" WeapNV9mmPistol": each filter is its number then its form, and the
    # removal form "OnAttachItemMod OnAttachScript 0" has two arguments; OnDetachItemMod
    # the same].
    "OnAttachItemMod": _item_mod_handler("runOnReconstruct"),
    "OnDetachItemMod": _item_mod_handler("runOnDeconstruct"),
    # [documented: the casino pages' examples, "GetCasinoChip GomorrahCasinoData",
    # "SetCasinoChip GomorrahCasinoData TheTopsChip", "GetCasinoWinnings TheTopsCasinoData",
    # "SetCasinoWinnings TheTopsCasinoData 5000"; the tables name the casino only or no type].
    "GetCasinoChip": [{"name": "casino", "type": "form", "kind": "form"}],
    "SetCasinoChip": [{"name": "casino", "type": "form", "kind": "form"},
                      {"name": "chip", "type": "form", "kind": "form"}],
    "GetCasinoWinnings": [{"name": "casino", "type": "form", "kind": "form"}],
    "SetCasinoWinnings": [{"name": "casino", "type": "form", "kind": "form"},
                          {"name": "chips", "type": "int", "kind": "int"}],
    # [documented: AddConditionFormTwo's example "AddConditionFormTwo StealthBoy 0" adds
    # to Base Effect 0's list; every other FormTwo function (GetConditionFormTwoCount,
    # DeleteConditionFormTwo, ...) takes `First` after the entity; the table leaves it out].
    # [documented: ToggleCraftingMessages' example "ToggleCraftingMessages 0 // Disables the
    # crafting messages"; the table gives the argument a type but no name, which the
    # compiler does not count].
    "ToggleCraftingMessages": [{"name": "enable", "type": "int", "kind": "int"}],
    # [documented: DisableAlt's example "TerminalRef.DisableAlt" passes nothing; the table
    # writes the optional mark as "Optional = Optional", which the parser does not read].
    "DisableAlt": [{"name": "FadeOut", "type": "Int", "kind": "int", "optional": True}],
    "AddConditionFormTwo": [{"name": "FormTwoEntity", "type": "ref", "kind": "reference"},
                            {"name": "First", "type": "int", "kind": "int"},
                            {"name": "Condition", "type": "int", "kind": "int",
                             "optional": True}],
}

# Extender functions with no wiki page, from their call shapes in a plugin's script
# source and what they did in the original (issue #976, menu-xml.md §10.2): MCM.dll's
# list functions [observed 2026-11-12: MCM 1.5's own scripts (SCTX) call them with these
# arguments; job mcm976e measured their effects]. No opcode: they go by name.
def _i(name):
    return {"name": name, "type": "int", "kind": "int"}


PAGELESS = [
    {"name": "GetMCMListWidth", "page": None, "origin": "MCM", "returns": {"type": "float", "name": None},
     "params": [_i("List"), _i("Row")], "categories": ["Functions (MCM)"]},
    {"name": "SetMCMFloatMass", "page": None, "origin": "MCM", "returns": {"type": "void", "name": None},
     "params": [_i("Child"), _i("First"), _i("Last"),
                {"name": "Setting", "type": "string", "kind": "string"},
                {"name": "Value", "type": "float", "kind": "float"}], "categories": ["Functions (MCM)"]},
    {"name": "SetMCMModList", "page": None, "origin": "MCM", "returns": {"type": "void", "name": None},
     "params": [_i("Row"), _i("Enable"), _i("ModIndex"),
                _i("Value"), {"name": "Form", "type": "form", "kind": "form"}],
     "categories": ["Functions (MCM)"]},
    {"name": "SetMCMStringMass", "page": None, "origin": "MCM", "returns": {"type": "void", "name": None},
     "params": [_i("Child"), _i("First"), _i("Last"),
                {"name": "Setting", "type": "string", "kind": "string"},
                {"name": "text", "type": "formattedString", "kind": "string"},
                {"name": "formatVars(up to 20)", "type": None, "kind": "unknown", "optional": True}],
     "categories": ["Functions (MCM)"]},
    # TTW NVSE (ttw_nvse.dll): "toggle radios working in child worldspaces on/off"
    # [documented: the TTW NVSE mod page's function list; the GECK wiki has no page].
    {"name": "TTW_EnableRadioFix", "page": None, "origin": "TTW", "returns": {"type": "void", "name": None},
     "params": [_i("enable")], "categories": ["Functions (TTW)", "Radio Functions"]},
]


# Commands JIP LN 57.30 registers (nvse.log RegisterCommand lines, opcodes 0x2212 and
# 0x26FD-0x2941) that have no wiki page, called by the Blue Moon and High and Dry lists'
# mods: their argument shapes from those mods' script source, what they do from probes in
# the original (jip_probes.py sets cmd1-cmd3; docs/spec-notes/ext-unlisted-commands.md).
def _p(name, kind, typ=None, optional=False):
    d = {"name": name, "type": typ or kind, "kind": kind}
    if optional:
        d["optional"] = True
    return d


def _jip(name, returns, params=(), ref="reference", cat="Functions (JIP)"):
    rec = {"name": name, "page": None, "origin": "JIP", "returns": {"type": returns, "name": None},
           "params": list(params), "categories": [cat, "Functions (JIP)"] if cat != "Functions (JIP)" else [cat]}
    if ref:
        rec["calling_reference"] = ref
    if not rec["params"]:
        del rec["params"]
    return rec


_FMT = [_p("text", "string", "formattedString"), _p("formatVars(up to 20)", "unknown", None, True)]
_FMT[1]["type"] = None
PAGELESS += [
    _jip("AttachLight", "successful:0/1", [_p("nodeName", "string"), _p("light", "form"), _p("x", "float"),
                                           _p("y", "float"), _p("z", "float")], cat="Model Functions (JIP)"),
    _jip("RemoveLight", "int", [_p("nodeName", "string")], cat="Model Functions (JIP)"),
    _jip("RefreshAnimData", "void", cat="Actor Functions (JIP)"),
    _jip("LockEquipment", "void", [_p("lock", "int")], cat="Actor Functions (JIP)"),
    _jip("GetTeammateUsingAmmo", "int", cat="Actor Functions (JIP)"),
    _jip("SetTeammateUsingAmmo", "void", [_p("usesAmmo", "int")], cat="Actor Functions (JIP)"),
    _jip("ToggleDetectionFix", "void", [_p("enable", "int")], cat="Actor Functions (JIP)"),
    _jip("ToggleIgnoreLockedDoors", "void", [_p("enable", "int")], cat="Actor Functions (JIP)"),
    _jip("MoveAwayFromPlayer", "void", cat="Actor Functions (JIP)"),
    _jip("KillActorAlt", "void", [_p("killer", "reference", "ref", True)], cat="Actor Functions (JIP)"),
    _jip("GetHitNode", "string_var", cat="Actor Functions (JIP)"),
    _jip("RefToPosStr", "string_var", cat="Misc Reference Functions (JIP)"),
    _jip("MoveToPosStr", "successful:0/1", [_p("position", "string")], cat="Misc Reference Functions (JIP)"),
    _jip("GetCenterPos", "float", [_p("axis", "axis", "X/Y/Z")], cat="Misc Reference Functions (JIP)"),
    _jip("DisableScriptedActivate", "void", [_p("disable", "int")], cat="Misc Reference Functions (JIP)"),
    _jip("GetConditionDamagePenalty", "float", ref=None, cat="Inventory Functions (JIP)"),
    _jip("ListToArray", "array", [_p("formList", "form")], ref=None, cat="Form List Functions (JIP)"),
    # The Companions Command & Control mod's own functions.
    _jip("CCCInFaction", "int", [_p("faction", "form"), _p("includeBase", "int", None, True)], cat="CCC Functions (JIP)"),
    _jip("CCCGetDistance", "float", [_p("target", "reference", "ref"), _p("mode", "int")], cat="CCC Functions (JIP)"),
    _jip("CCCLocationName", "string_var", cat="CCC Functions (JIP)"),
    _jip("CCCGetReputation", "ref", cat="CCC Functions (JIP)"),
    _jip("CCCSavedForm", "ref", [_p("index", "int")], ref=None, cat="CCC Functions (JIP)"),
    _jip("CCCSetFloat", "void", [_p("index", "int"), _p("value", "float")], ref=None, cat="CCC Functions (JIP)"),
    _jip("CCCSetString", "void", [_p("index", "int")] + [dict(p) for p in _FMT], ref=None, cat="CCC Functions (JIP)"),
    _jip("CCCSetTrait", "void", [_p("index", "int"), _p("value", "float"), _p("value2", "float", None, True)], ref=None,
         cat="CCC Functions (JIP)"),
    _jip("CCCSetEquipped", "int", cat="CCC Functions (JIP)"),
    _jip("CCCSetNCCS", "void", [_p("enable", "int")], cat="CCC Functions (JIP)"),
    _jip("CCCSMS", "void", [_p("actor", "reference", "ref")], ref=None, cat="CCC Functions (JIP)"),
    _jip("CCCSetFollowState", "int", [_p("state", "int", None, True)], cat="CCC Functions (JIP)"),
    _jip("CCCSayTo", "void", [_p("target", "reference", "ref"), _p("topic", "form")], cat="CCC Functions (JIP)"),
    _jip("CCCRunResultScripts", "void", [_p("topic", "form")], cat="CCC Functions (JIP)"),
    _jip("CCCLoadNCCS", "void", ref=None, cat="CCC Functions (JIP)"),
    _jip("CCCOnLoad", "void", ref=None, cat="CCC Functions (JIP)"),
]



# The rest of the registered commands no wiki page lists (#2326), batch 1: argument counts
# and kinds from the compile-shape run in the original (each name compiled with 23 argument
# patterns inside `if 0`; docs/spec-notes/ext-unlisted-rest.md §1), behaviour from set rest1.
def _x(name, origin, returns, params=(), ref=None, aliases=None, cat=None):
    rec = {"name": name, "page": None, "origin": origin, "returns": {"type": returns, "name": None},
           "params": list(params), "categories": [cat or f"Functions ({origin})"]}
    if aliases:
        rec["aliases"] = aliases
    if ref:
        rec["calling_reference"] = ref
    if not rec["params"]:
        del rec["params"]
    return rec


_S, _I, _F, _R = (lambda n, o=False: _p(n, "string", None, o)), (lambda n, o=False: _p(n, "int", None, o)), \
    (lambda n, o=False: _p(n, "float", None, o)), (lambda n, o=False: _p(n, "reference", "ref", o))
_A = lambda n, o=False: _p(n, "array", "array_var", o)  # noqa: E731
_SO = "ShowOff"
PAGELESS += [
    # SUP's network services, tests and log (answer 0, extender-compat.md §5.8).
    _x("BBAPIKeyExist", "SUP", "int"), _x("BBAreMessagesFetched", "SUP", "int"), _x("BBDeleteUserMessage", "SUP", "int", [_I("id")]),
    _x("BBGetBBKey", "SUP", "string_var"), _x("BBGetCurrentNotesInCell", "SUP", "array"),
    _x("BBGetInfoForReference", "SUP", "array", [_R("ref")]), _x("BBGetTextMenuString", "SUP", "string_var"),
    _x("BBGetUserInfo", "SUP", "array", [_I("id")]), _x("BBGetUserMessages", "SUP", "array"), _x("BBHideMessage", "SUP", "int", [_I("id")]),
    _x("BBIsBBLoaded", "SUP", "int"), _x("BBIsReferenceNote", "SUP", "int", [_R("ref")]), _x("BBIsTextEditMenuActive", "SUP", "int"),
    _x("BBLikeMessage", "SUP", "int", [_I("id")]), _x("BBNexusAPIKeyExist", "SUP", "int"), _x("BBOnCellChange", "SUP", "void", [_R("cell")]),
    _x("BBPostMessage", "SUP", "int", [_S("text"), _R("ref")]), _x("BBReportMessage", "SUP", "int", [_I("id"), _S("reason")]),
    _x("BBReturnAllMessagesToDefaultCell", "SUP", "void"), _x("BBSetTextMenuTitle", "SUP", "void", [_S("title")]),
    _x("UpdateCheckerCheckForUpdates", "SUP", "int", [_I("a"), _I("b"), _S("url")]),
    _x("UpdateCheckerReturnArray", "SUP", "array", [_I("id")]),
    _x("GetSUPDateCheckerCurrentStage", "SUP", "int"), _x("GetSUPDateCheckerDoneStage", "SUP", "int"),
    _x("GetPastebinReaderCurrentStage", "SUP", "int"), _x("GetPastebinReaderDoneStage", "SUP", "int"),
    _x("SUPTest", "SUP", "void"), _x("LunetteCMD", "SUP", "void", [_I("command")]), _x("SUPWRITETOLOG", "SUP", "void", [_S("text")]),
    _x("DumpTileInfoChilds", "SUP", "void", [_S("tile", True)]),
    _x("StringContainsOnlyASCII", "SUP", "int", [_S("text")]), _x("IsNewGame", "SUP", "int"),
    _x("ERDTestFunc", "One111", "void", [_I("value")]),
    _x("GetFuncCodeByName", "One111", "int", [_S("name")]), _x("GetFuncNameByCode", "One111", "string_var", [_I("code")]),
    _x("NullArgs", "JohnnyGuitar", "void", [_I("value", True)]), _x("RefAddrxData", "JohnnyGuitar", "int", [_I("type")]),
    _x("kNVSEReset", "kNVSE", "void"),
    # ShowOff's `_OLD` commands, answering as the current ones.
    _x("Matrix_IsMatrix_OLD", _SO, "int", [_A("array")], aliases=["Mat_IsMat_OLD"]),
    _x("Matrix3x3_GetQuaternion_OLD", _SO, "array", [_A("3x3RotationMatrix")], aliases=["Mat_GetQuat_OLD"]),
    _x("Quaternion_GetMatrix_OLD", _SO, "array", [_A("quaternion")], aliases=["Quat_GetMat_OLD"]),
    _x("Matrix_Transpose_OLD", _SO, "array", [_A("matrix")], aliases=["Mat_T_OLD"]),
    _x("Matrix_Dump_OLD", _SO, "void", [_A("matrix")], aliases=["Mat_Dump_OLD"]),
    _x("Matrix_ApplyOperationWithScalar_OLD", _SO, "array", [_A("matrix"), _S("operator"), _F("scalar")], aliases=["Mat_ApplyOpWithScal_OLD"]),
    _x("Matrix_ApplyOperationWithMatrix_OLD", _SO, "array", [_A("matrixA"), _S("operator"), _A("matrixB")], aliases=["Mat_ApplyOpWithMat_OLD"]),
    _x("ListAddArray_OLD", _SO, "int", [_p("formlist", "form"), _A("array"), _I("index", True)], aliases=["AddArrayToFormList_OLD"]),
    _x("TopicInfoSetResponseStrings_OLD", _SO, "void", [_p("TopicInfo", "form"), _A("newStrings")]),
    _x("IsJumping_BROKEN", _SO, "int", ref="reference"),
    # ShowOff's string maps.
    _x("AuxStringMapArrayGetSize", _SO, "int", [_S("name")], aliases=["AuxStringMapSize"]),
    _x("AuxStringMapArrayGetValueType", _SO, "int", [_S("name"), _S("key")], aliases=["AuxStringMapGetValueType"]),
    _x("AuxStringMapArrayGetFloat", _SO, "float", [_S("name"), _S("key")], aliases=["AuxStringMapGetFlt"]),
    _x("AuxStringMapArrayGetRef", _SO, "ref", [_S("name"), _S("key")], aliases=["AuxStringMapGetRef"]),
    _x("AuxStringMapArrayGetString", _SO, "string_var", [_S("name"), _S("key")], aliases=["AuxStringMapGetStr"]),
    _x("AuxStringMapArrayGetFirst", _SO, "array", [_S("name")], aliases=["AuxStringMapFirst"]),
    _x("AuxStringMapArrayGetNext", _SO, "array", [_S("name")], aliases=["AuxStringMapNext"]),
    _x("AuxStringMapArrayGetKeys", _SO, "array", [_S("name")], aliases=["AuxStringMapKeys"]),
    _x("AuxStringMapArrayGetAll", _SO, "array", [_I("temporary")], aliases=["AuxStringMapGetAll"]),
    _x("AuxStringMapArrayGetAsArray", _SO, "array", [_S("name")], aliases=["AuxStringMapGetAsArr"]),
    _x("AuxStringMapArraySetFromArray", _SO, "int", [_S("name"), _A("array"), _I("keep", True)], aliases=["AuxStringMapSetFromArr"]),
    _x("AuxStringMapArraySetFloat", _SO, "int", [_S("name"), _S("key"), _F("value")], aliases=["AuxStringMapSetFlt"]),
    _x("AuxStringMapArraySetRef", _SO, "int", [_S("name"), _S("key"), _R("value")], aliases=["AuxStringMapSetRef"]),
    _x("AuxStringMapArraySetString", _SO, "int", [_S("name"), _S("key"), _S("value")], aliases=["AuxStringMapSetStr"]),
    _x("AuxStringMapArrayEraseKey", _SO, "int", [_S("name"), _S("key")], aliases=["AuxStringMapEraseKey"]),
    _x("AuxStringMapArrayValidateValues", _SO, "int", [_S("name")], aliases=["AuxStringMapValidateVals"]),
    _x("AuxStringMapArrayDestroy", _SO, "int", [_S("name")], aliases=["AuxStringMapDestroy"]),
]



# Batch 2 (#2326): SUP's JSON commands, shapes from the compile-shape run, behaviour from
# sets json1 and json2 (docs/spec-notes/ext-unlisted-rest.md §6).
_JP = lambda: _S("path")  # noqa: E731
PAGELESS += [
    _x("JSONCacheFile", "SUP", "int", [_JP(), _I("flag")]),
    _x("JSONCacheFileFromString", "SUP", "int", [_S("json"), _S("path")]),
    _x("JSONIsFileCached", "SUP", "int", [_JP()]),
    _x("JSONToString", "SUP", "string_var", [_JP()]),
    _x("JSONCloseFile", "SUP", "int", [_JP(), _I("flag"), _I("extra", True)]),
    _x("JSONGetValueType", "SUP", "int", [_JP(), _S("key"), _I("cached")]),
    _x("JSONGetValueArray", "SUP", "array", [_JP(), _S("key"), _I("cached")]),
    _x("JSONSetValueFloat", "SUP", "int", [_JP(), _S("key"), _F("value"), _I("cached"), _I("extra", True)]),
    _x("JSONSetValueString", "SUP", "int", [_JP(), _S("key"), _S("value"), _I("cached")]),
    _x("JSONAppendValueFloat", "SUP", "int", [_JP(), _S("key"), _F("value"), _I("cached"), _I("extra", True)]),
    _x("JSONAppendValueString", "SUP", "int", [_JP(), _S("key"), _S("value"), _I("cached"), _I("extra", True)]),
    _x("JSONEraseArrayElement", "SUP", "int", [_JP(), _S("key"), _I("index"), _I("cached")]),
    _x("JSONClearKey", "SUP", "int", [_JP(), _S("key"), _I("cached")]),
    _x("JSONEraseKey", "SUP", "int", [_JP(), _S("key"), _I("cached")]),
]



# Batch 3 (#2326): kNVSE's animation commands, shapes from the compile-shape run,
# behaviour from sets knv1-knv3 (docs/spec-notes/ext-unlisted-rest.md §7).
_KF = lambda: _S("animPath")  # noqa: E731
_KN = "kNVSE"
PAGELESS += [
    _x("GetAnimPathBySequenceType", _KN, "string_var", [_I("sequenceType"), _I("flag", True)], ref="reference", aliases=["GetAnimPathByType"]),
    _x("GetAnimWeight", _KN, "float", [_KF()], ref="reference"),
    _x("GetAnimHandType", _KN, "int", [_KF()], ref="reference"),
    _x("IsAnimAdditive", _KN, "int", [_KF()], ref="reference"),
    _x("IsAnimQueued", _KN, "int", [_KF()], ref="reference"),
    _x("GetAnimInterpPriority", _KN, "int", [_KF(), _S("node")], ref="reference"),
    _x("SetAnimInterpPriority", _KN, "int", [_KF(), _S("node"), _I("priority")], ref="reference"),
    _x("GetAnimTextKeyTime", _KN, "float", [_KF(), _S("textKey")], ref="reference"),
    _x("RemoveAnimTextKey", _KN, "int", [_KF(), _I("index")], ref="reference"),
    _x("CopyAnimPriorities", _KN, "int", [_KF(), _S("fromPath")], ref="reference"),
    _x("SetAnimCycleType", _KN, "int", [_KF(), _I("cycleType"), _I("flag", True)], ref="reference"),
    _x("SetAnimDestFrame", _KN, "int", [_KF(), _F("frame"), _I("flag", True)], ref="reference"),
    _x("SetAnimAdditiveInterpsWeightMult", _KN, "int", [_I("a"), _I("b"), _I("c"), _F("mult"), _I("flag", True)], ref="reference"),
    _x("QueueNextAnim", _KN, "int", [_KF(), _I("flag", True)], ref="reference"),
    _x("RegisterCustomAnimGroup", _KN, "int", [_I("group"), _S("name")]),
    _x("EraseCurrentAnimAction", _KN, "int", ref="reference"),
    _x("EjectWeapon", _KN, "int", ref="reference"),
    _x("ThisAnim", _KN, "string_var"),
    _x("ThisAnimDir", _KN, "string_var"),
    _x("CreateIdleAnimForm", _KN, "ref", [_KF(), _I("flag", True)]),
    _x("PlayGroupAlt", _KN, "int", [_S("group"), _I("flag")], ref="reference"),
    _x("GetDirectoryFiles", _KN, "array", [_S("pattern"), _I("flag", True)]),
]



# Batch 4 (#2326): ShowOff, SUP, One111, ITR and JohnnyGuitar's small families; shapes
# from the compile-shape run (a shape no pattern compiled gets optional numbers),
# behaviour from set rest4 (docs/spec-notes/ext-unlisted-rest.md §8).
_ANY = [_p(f"arg{i}", "unknown", None, True) for i in range(1, 7)]
for _a in _ANY:
    _a["type"] = None
_IT, _JG, _O1 = "ITR", "JohnnyGuitar", "One111"
PAGELESS += [
    _x("GetActorCoverState", _IT, "int", ref="reference"), _x("GetActorCoverInfo", _IT, "array", ref="reference"),
    _x("GetCurrentDialogue", _IT, "int"), _x("GetEligiblePerksEx", _IT, "array", [_R("actor"), _I("flag", True)]),
    _x("SetDismemberedLimbVelocity", _IT, "int", [_I("limb"), _F("x"), _F("y"), _F("z"), _I("a", True), _I("b", True)], ref="reference"),
    _x("SpawnTempParticle", _IT, "int", [_F(f"v{i}") for i in range(1, 10)]),
    _x("GetCameraShotPath", _JG, "int", [_p("cameraShot", "form")]),
    _x("SetCameraShotPath", _JG, "int", [_p("cameraShot", "form"), _S("path")]),
    _x("GetCasinoDeckTexture", _JG, "string_var", [_p("casino", "form"), _I("deck", True)]),
    _x("SetCasinoDeckTexture", _JG, "int", [_p("casino", "form"), _I("deck", True), _I("flag", True)]),
    _x("GetExtraAccuracyPenaltyMult", _JG, "float", [_S("key", True)], ref="reference"),
    _x("SetExtraAccuracyPenaltyMult", _JG, "int", [_F("mult"), _S("key", True)], ref="reference"),
    _x("RemoveExtraAccuracyPenaltyMult", _JG, "int", [_S("key", True)], ref="reference"),
    _x("GetSwitchNodeIndex", _JG, "int", [_S("node"), _I("flag", True)], ref="reference"),
    _x("SetMusicTypePath", _JG, "int", [_p("musicType", "form"), _S("path")]),
    _x("SetMediaLocationControllerOverride", _JG, "int", [_p("controller", "form")], aliases=["SetMLCOverride"]),
    _x("ClearMediaLocationControllerOverride", _JG, "int", aliases=["ClearMLCOverride"]),
    _x("GetPackedPlayerFOV", _JG, "int", list(_ANY)), _x("JGLegacyWorldToScreen", _JG, "int", list(_ANY)),
    _x("SetBlockTransform", _JG, "int", list(_ANY)), _x("SetDialogResponseOverrideValues", _JG, "int", list(_ANY)),
    _x("GetWeapAdditionalFloat", _O1, "float", [_p("weapon", "form"), _I("which")]),
    _x("SetWeapAdditionalFloat", _O1, "int", [_p("weapon", "form"), _I("which"), _F("value")]),
    _x("SetWeapAdditionalFlag", _O1, "int", [_p("weapon", "form"), _I("flag"), _I("on")]),
    _x("SetOnDamageCalcFlag", _O1, "int", [_I("flag")]), _x("AddPerkEntry", _O1, "int", list(_ANY)),
    _x("IsTemporaryReference", _SO, "int", ref="reference", aliases=["IsTempRefr"]),
    _x("GetWorldOffsetPosArray", _SO, "array", ref="reference"),
    _x("GetLevelUpMenuUnspentPoints", _SO, "int", [_I("which")]), _x("SetLevelUpMenuPoints", _SO, "int", [_I("which"), _I("points")]),
    _x("GetPipboyRadioVoiceEntryData", _SO, "array", [_I("index"), _I("flag", True)]),
    _x("GetNoEquipShowOff", _SO, "int", [_p("item", "form"), _I("flag")], aliases=["GetNoEquipSO"]),
    _x("SetNoEquipShowOff", _SO, "int", [_p("item", "form"), _I("noEquip")], aliases=["SetNoEquipSO"]),
    _x("SayTo_GetSpeakingActor", _SO, "ref"), _x("SayTo_GetQuest", _SO, "ref"),
    _x("GetLiveEffectBaseTrait", _SO, "float", [_I("trait")]),
    _x("SetLiveEffectMagnitudeModifier", _SO, "int", [_I("effect"), _F("modifier")]),
    _x("ShowingOffDisable", _SO, "int", [_I("value", True)], aliases=["DisableIFYouDidntNotice"]),
    _x("ShowingOffEnable", _SO, "int", [_I("value", True)], aliases=["EnableIFYouDidntNotice"]),
    _x("MessageExAltShowoff", _SO, "int", [_F("time"), _I("flag")] + [dict(p) for p in _FMT]),
    _x("BaseCopyFaceGenFrom", _SO, "int", [_p("to", "form"), _p("from", "form")]),
    _x("SetFactionCombatReactionTemp", _SO, "int", list(_ANY)),
    _x("CompareNifBlocks", "SUP", "array", [_S("node"), _I("flag")], ref="reference"),
    _x("GetNifChildrenIDsByName", "SUP", "array", [_S("node")], ref="reference"),
    _x("GetNifChildrenIDsByID", "SUP", "array", [_I("id"), _I("flag")], ref="reference"),
    _x("GetNifTypeByID", "SUP", "int", [_I("id"), _I("flag")], ref="reference"),
    _x("GetNifStringByID", "SUP", "string_var", [_I("id"), _I("flag")], ref="reference"),
    _x("SUPGetConditionVarFloat", "SUP", "float", [_I("index")]),
    _x("TopicGetTopicInfoQuest", "SUP", "ref", [_p("info", "form")]),
    _x("HudBarSetForceHide", "SUP", "int", [_I("bar"), _I("hide"), _S("name")], aliases=["HBSetForceHide"]),
    _x("HudBarGetPos", "SUP", "int", list(_ANY), aliases=["HBGetPos"]),
    _x("GetCalculatedPosFrame", "SUP", "int", list(_ANY)), _x("GetNifBlockTranslationToVars", "SUP", "int", list(_ANY)),
    _x("TopicReplaceTopicInfo", "SUP", "int", list(_ANY)),
]



# Batch 5 (#2326): JIP's unlisted commands; shapes from the compile-shape run, behaviour
# from set rest5 (docs/spec-notes/ext-unlisted-rest.md §9).
_J = "JIP"
PAGELESS += [
    _x("GetRaceAgeRace", _J, "ref", [_p("race", "form"), _I("younger")]), _x("SetRaceAgeRace", _J, "int", [_p("race", "form"), _I("younger"), _p("ageRace", "form")]),
    _x("GetRaceVoice", _J, "ref", [_p("race", "form"), _I("female")]), _x("SetRaceVoice", _J, "int", [_p("race", "form"), _I("female"), _p("voice", "form")]),
    _x("GetScriptBlockDisabled", _J, "int", [_p("script", "form"), _I("block")]),
    _x("SetScriptBlockDisabled", _J, "int", [_p("script", "form"), _I("block"), _I("disable", True)]),
    _x("GetBodyPartVATSTarget", _J, "string_var", [_p("bodyPartData", "form"), _I("part", True)]),
    _x("GetObjectiveTeleportLinks", _J, "array", [_p("quest", "form"), _I("objective")]),
    _x("CalculateShotsPerSec", _J, "float", [_p("weapon", "form")]),
    _x("GetEquippedArmorRefs", _J, "array", ref="reference"), _x("GetCollisionNodes", _J, "array", ref="reference"),
    _x("HasActionRef", _J, "int", ref="reference"), _x("IsAttackQueued", _J, "int", ref="reference"),
    _x("GetQuestTargetsChanged", _J, "int"), _x("GetRecipeMenuNumSubcategories", _J, "int"),
    _x("RefMapArrayGetValue", _J, "array", [_S("name"), _R("key", True)], aliases=["RefMapGetVal"]),
    _x("RefMapArraySetValue", _J, "int", [_S("name"), _p("value", "unknown"), _R("key", True)], aliases=["RefMapSetVal"]),
    _x("GetArrayValue", _J, "unknown", [_A("array"), _I("index")]),
    _x("SetContainerOpenSound", _J, "int", [_p("sound", "form"), _p("container", "form", None, True)]),
    _x("SetContainerCloseSound", _J, "int", [_p("sound", "form"), _p("container", "form", None, True)]),
    _x("SetWaterTrait", _J, "int", [_p("water", "form"), _I("trait"), _F("value")]),
    _x("SetWindSpeedMult", _J, "int", [_F("mult")]), _x("ToggleDepthClear", _J, "int", [_I("enable")]),
    _x("ToggleHitEffects", _J, "int", [_I("enable", True)], ref="reference"),
    _x("ToggleNoMovementCombat", _J, "int", [_I("enable", True)], ref="reference"),
    _x("ToggleNoZPosReset", _J, "int", [_I("enable")], ref="reference"),
    _x("ToggleNodeCollision", _J, "int", [_S("node"), _I("enable")], ref="reference"),
    _x("SuppressQuestMessages", _J, "int"), _x("UnlockRecipeMenuQuantity", _J, "int", [_I("unlock", True)]),
    _x("SetItemFilter", _J, "int", [_S("filter", True)]), _x("InitItemFilter", _J, "int"),
    _x("SetTextureTransformKey", _J, "int", [_I("a"), _I("b"), _I("c"), _F("value")], ref="reference", aliases=["SetTexTransKey"]),
    _x("ClearModelAltTextures", _J, "int", [_p("form", "form")]), _x("InitRockItLauncher", _J, "int", [_p("weapon", "form")]),
    _x("EnableImprovedRecipeMenu", _J, "int"), _x("SwapObjectLOD", _J, "int", [_p("object", "form"), _I("a"), _I("b")]),
    _x("ScrollMouseWheel", _J, "int", [_I("delta", True)]), _x("InitMiniMap", _J, "int"),
    _x("UpdateMiniMap", _J, "int", [_I("a"), _I("b", True)]), _x("ProjectUITile", _J, "int", [_F(f"v{i}") for i in range(1, 7)]),
    _x("ReloadCharController", _J, "int", ref="reference"), _x("ReloadCloudTextures", _J, "int"), _x("ResetClouds", _J, "int"),
    _x("ExecuteScript", _J, "int"), _x("ShowLevelUpMenuEx", _J, "int", [_I("mode")]),
    _x("CCCTaskPackageFlags", _J, "int", list(_ANY)), _x("GetCameraMovement", _J, "string_var", list(_ANY)),
    _x("GetTranslatedPos", _J, "int", list(_ANY)), _x("GetWorldMapPosMults", _J, "int", list(_ANY)),
    _x("TransformWorldToLocal", _J, "int", list(_ANY)),
]


# A short command of xNVSE's with no page and no RegisterCommand line, called by a mod's
# loose script (B42 Interact's ln_B42Interact.txt: `if GPCDEx 2 4 && GPCDEx 2 8 ...`):
# it compiled in the original and answered as GetPlayerControlsDisabledAltEx does,
# `disabledHow flags` [observed: #2430 probe 2026-11-14, Blue Moon sandbox: 2 4 gave 0,
# 1 after DisablePlayerControlsAltEx 4, 0 after EnablePlayerControlsAltEx 4; 2 8 gave 0
# and 1 4 gave 1 while 4 was disabled].
PAGELESS += [
    _x("GPCDEx", "NVSE", "int", [_I("disabledHow", True), _I("flags", True)]),
]


# Pages whose template `name` is another function's (copied from a sibling page): the
# page title is the function's name [documented: each page's title and its example,
# e.g. "GetConditionFormOneFloat HitReactionIdles 0 1"; the template says
# GetConditionFormOneInt / SetConditionFormOneInt].
NAME_FIXES = {
    # The template's name has a space for the underscore, so only "Ar" was kept
    # [documented: the page's link, Ar_SortEditor; observed 2026-11-08: a variable aR
    # compiles in the original with JohnnyGuitar loaded, so no command is named Ar].
    "Ar SortEditor": "Ar_SortEditor",
    "GetConditionFormOneFloat": "GetConditionFormOneFloat",
    "SetConditionFormOneFloat": "SetConditionFormOneFloat",
    "SetConditionFormOneForm": "SetConditionFormOneForm",
    # The template's name has a space for the underscore, so only "Matrix" was kept
    # [documented: the page's title and its link, Matrix_ApplyOperationWithScalar].
    "Matrix ApplyOperationWithScalar": "Matrix_ApplyOperationWithScalar",
}


def parse_page(title, text):
    body = _template_body(text, "Function")
    if body is None:
        return None
    f = template_fields(body)
    name = (plain(f.get("name", "")) or title).split()[0]
    name = NAME_FIXES.get(title, name)
    rec = {"name": name, "page": title, "origin": f.get("origin", "").strip() or None}
    if f.get("alias"):
        rec["aliases"] = [a for a in (x.strip() for x in re.split(r"[,/]| or ",
                                                                  plain(f["alias"])))
                          if a and a.lower() != name.lower() and IDENT.fullmatch(a)]
    if f.get("originVersion"):
        rec["origin_version"] = plain(f["originVersion"])
    if f.get("referenceType"):
        rec["calling_reference"] = plain(f["referenceType"])
    rtype, rname = plain(f.get("returnType", "")), plain(f.get("returnVal", ""))
    if rtype or rname:
        rec["returns"] = {"type": rtype or None, "name": rname or None}
    params = []
    for m in re.finditer(r"\{\{\s*FunctionArgument(.*?)\}\}", f.get("arguments", ""), re.S):
        af = template_fields(m.group(1))
        raw = param_type(plain(af.get("Type", "")) or None)
        # `{{FunctionArgument |Name = |Type = }}`: the page's empty placeholder for a
        # function that takes nothing (GetCallingScript, KillAll2, ...), not an argument.
        if raw is None and not plain(af.get("Name", "")):
            continue
        p = {"name": plain(af.get("Name", "")), "type": raw, "kind": arg_kind(raw)}
        if _truthy(af.get("Optional", af.get("optional", ""))):
            p["optional"] = True
        params.append(p)
    if name in PARAM_FIXES:
        params = [dict(x) for x in PARAM_FIXES[name]]
    if params:
        rec["params"] = params
    cond = plain(f.get("conditionFunc", "")).split()
    if cond:
        rec["wiki_condition_func"] = cond[0]
    if _truthy(f.get("consoleOnly", "")):
        rec["console_only"] = True
    rec["categories"] = sorted({c.strip().replace("_", " ")
                                for c in re.findall(r"\[\[Category:([^\]|]+)", text)})
    return rec


# --------------------------------------------------------------------------- xEdit table

def parse_xedit(path):
    """Condition-function table: index -> (name, [parameter type names])."""
    table = {}
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            m = re.match(r"\s*\(Index:\s*(\d+);\s*Name:\s*'([^']+)'(.*)\)", line)
            if m:
                table[int(m.group(1))] = (m.group(2), re.findall(r"ParamType\d:\s*(\w+)",
                                                                 m.group(3)))
    return table


def xedit_from_catalogue(path):
    """The same table rebuilt from an earlier catalogue's `condition_index` and
    `xedit_param_types` (for regenerating without a copy of the xEdit source)."""
    with open(path, encoding="utf-8") as fh:
        doc = json.load(fh)
    return {f["condition_index"]: (f["name"], list(f.get("xedit_param_types") or []))
            for f in doc["functions"] if f.get("condition_index") is not None}


# --------------------------------------------------------------------------- plugins

def iter_script_units(buf, plugin):
    """Yield embedded/standalone scripts (SCHR..SCDA..SCTX) and CTDA function indices."""
    pos, n = 0, len(buf)
    while pos + 24 <= n:
        sig = buf[pos:pos + 4]
        size, flags, form_id = struct.unpack_from("<III", buf, pos + 4)
        if sig == b"GRUP":
            pos += 24
            continue
        data = buf[pos + 24:pos + 24 + size]
        pos += 24 + size
        if flags & 0x00040000:
            try:
                data = zlib.decompressobj().decompress(data[4:])
            except zlib.error:
                continue
        rtype = sig.decode("ascii", "replace")
        i, big, cur = 0, None, None
        while i + 6 <= len(data):
            sub = data[i:i + 4]
            sz = struct.unpack_from("<H", data, i + 4)[0]
            i += 6
            if sub == b"XXXX":
                big = struct.unpack_from("<I", data, i)[0]
                i += sz
                continue
            if big is not None:
                sz, big = big, None
            body = data[i:i + sz]
            i += sz
            if sub == b"SCHR":
                if cur:
                    yield cur
                cur = {"plugin": plugin, "record": rtype, "form_id": form_id,
                       "scda": b"", "sctx": ""}
            elif sub == b"SCDA" and cur is not None:
                cur["scda"] = body
            elif sub == b"SCTX" and cur is not None:
                cur["sctx"] = body.rstrip(b"\0").decode("cp1252", "replace")
            elif sub == b"CTDA" and len(body) >= 10:
                yield {"ctda": struct.unpack_from("<H", body, 8)[0], "record": rtype}
        if cur:
            yield cur


def statements(scda):
    """Top-level statements: (opcode, body, reference_prefixed). None if malformed."""
    out, i, ref = [], 0, False
    while i < len(scda):
        if i + 4 > len(scda):
            return None
        op, ln = struct.unpack_from("<HH", scda, i)
        if op == 0x1C:  # reference prefix: u16 reference index, then the statement
            ref = True
            i += 4
            continue
        out.append((op, scda[i + 4:i + 4 + ln], ref))
        ref = False
        i += 4 + ln
        if i > len(scda):
            return None
    return out


def expression_calls(body):
    """Function opcodes inside an expression body ('X' + u16 opcode). Heuristic scan."""
    found, i = [], 0
    while True:
        i = body.find(b"X", i)
        if i < 0 or i + 3 > len(body):
            return found
        op = struct.unpack_from("<H", body, i + 1)[0]
        if 0x1000 <= op < 0x1800:
            found.append(op)
        i += 1


def source_lines(sctx):
    """Source lines that compile to one statement each (no blanks/comments/declarations)."""
    out = []
    for line in sctx.replace("\r", "").split("\n"):
        line = re.sub(r";.*", "", line).strip()
        if line and not DECLARATION.match(line):
            out.append(line)
    return out


def scan_masters(data_dir, canonical):
    """Collect opcode evidence and usage.

    canonical maps a lowercase identifier (documented name or alias) to its function key.
    Top-level call statements give the called name directly (first identifier of the
    aligned source line). Calls inside expressions are attributed by line co-occurrence:
    per opcode, the function key with the highest Jaccard index over expression lines.
    """
    top = collections.defaultdict(collections.Counter)      # opcode -> first identifier
    line_ops = collections.Counter()
    line_tok = collections.Counter()
    line_co = collections.defaultdict(collections.Counter)
    calls = collections.Counter()
    scripts_with = collections.defaultdict(set)
    begin = collections.defaultdict(collections.Counter)
    stmt_counts = collections.Counter()
    ctda = collections.Counter()
    stats = collections.Counter()
    spelled = collections.Counter()                          # (key, source spelling)
    for plugin in MASTERS:
        path = os.path.join(data_dir, plugin)
        if not os.path.exists(path):
            continue
        with open(path, "rb") as fh:
            buf = fh.read()
        for unit in iter_script_units(buf, plugin):
            if "ctda" in unit:
                ctda[unit["ctda"]] += 1
                continue
            if not unit["scda"] or not unit["sctx"]:
                continue
            stats["scripts"] += 1
            stmts = statements(unit["scda"])
            if stmts is None:
                stats["malformed"] += 1
                continue
            lines = source_lines(unit["sctx"])
            aligned = len(lines) == len(stmts)
            stats["aligned" if aligned else "unaligned"] += 1
            key = (unit["plugin"], unit["form_id"], unit["record"])
            for idx, (op, body, _ref) in enumerate(stmts):
                stmt_counts[op] += 1
                if op >= 0x1000:
                    calls[op] += 1
                    scripts_with[op].add(key)
                if op == 0x10 and len(body) >= 2 and aligned:
                    m = re.match(r"begin\s+([A-Za-z]+)", lines[idx], re.I)
                    if m:
                        begin[struct.unpack_from("<H", body, 0)[0]][m.group(1)] += 1
                if not aligned:
                    continue
                line = lines[idx]
                if op >= 0x1000:
                    m = re.match(r"^(?:[A-Za-z0-9_]+\s*\.\s*)?([A-Za-z_][A-Za-z0-9_]*)", line)
                    if m:
                        top[op][m.group(1)] += 1
                elif op in EXPRESSION_STATEMENTS:
                    ops = set(expression_calls(body))
                    toks = IDENT.findall(re.sub(r'"[^"]*"', "", line))
                    keys = set()
                    for t in toks:
                        k = canonical.get(t.lower())
                        if k:
                            keys.add(k)
                            spelled[(k, t)] += 1
                        else:
                            keys.add("?" + t.lower())
                            spelled[("?" + t.lower(), t)] += 1
                    line_ops.update(ops)
                    line_tok.update(keys)
                    for o in ops:
                        calls[o] += 1
                        scripts_with[o].add(key)
                        line_co[o].update(keys)
    # Expression evidence: the documented function with the best line-level Jaccard index;
    # an undocumented identifier only when no documented one reaches 0.5 and it reaches 0.9.
    expr = {}
    for o, n in line_ops.items():
        scored = sorted(((c / (n + line_tok[k] - c), k, c) for k, c in line_co[o].items()),
                        reverse=True)
        known = [s for s in scored if not s[1].startswith("?")]
        if known and known[0][0] >= 0.5:
            j, k, c = known[0]
        elif scored and scored[0][0] >= 0.9:
            j, k, c = scored[0]
        else:
            continue
        spell = max((s for s in spelled if s[0] == k), key=lambda s: spelled[s])[1]
        expr[o] = {"key": k, "spelling": spell, "lines": c, "jaccard": round(j, 3)}
    return {"top": top, "expr": expr, "calls": calls,
            "scripts": {o: len(s) for o, s in scripts_with.items()},
            "begin": begin, "statements": stmt_counts, "ctda": ctda, "stats": stats}


# --------------------------------------------------------------------------- merge

def canonical_map(wiki, xedit):
    """Lowercase identifier -> function key (lowercase documented name)."""
    canon = {}
    for rec in wiki:
        for n in [rec["name"], rec["page"]] + rec.get("aliases", []):
            canon.setdefault(n.lower(), rec["name"].lower())
    for name, _ in xedit.values():
        canon.setdefault(name.lower(), name.lower())
    return canon


MIN_SPELLING_USES = 3   # an undocumented spelling needs this many aligned call lines


def resolve_opcodes(scan, canon, xedit):
    """Opcode -> function key, plus spellings, undocumented names and leftovers."""
    resolved, spellings, extra, conflicts = {}, collections.defaultdict(collections.Counter), {}, []
    for op in sorted(set(scan["top"]) | set(scan["expr"])):
        known, unknown = collections.Counter(), collections.Counter()
        shown = {}
        for tok, c in scan["top"].get(op, {}).items():
            k = canon.get(tok.lower())
            if k:
                known[k] += c
                spellings[k][tok] += c
            else:
                shown.setdefault(tok.lower(), tok)
                unknown[shown[tok.lower()]] += c
        e = scan["expr"].get(op)
        if e and not e["key"].startswith("?"):
            known[e["key"]] += e["lines"]
            spellings[e["key"]][e["spelling"]] += e["lines"]
        elif e:
            unknown[e["spelling"]] += e["lines"]
        if known:
            key = known.most_common(1)[0][0]
            for tok, c in unknown.items():
                if c >= MIN_SPELLING_USES:
                    spellings[key][tok] += c
            noise = {k: c for k, c in known.items() if k != key}
            noise.update({t: c for t, c in unknown.items() if c < MIN_SPELLING_USES})
            if noise:
                conflicts.append({"opcode": f"0x{op:04X}", "chosen": key,
                                  "chosen_lines": known[key], "other_tokens": noise})
        else:
            xname = xedit.get(op - 0x1000, (None,))[0]
            tok = unknown.most_common(1)[0][0]
            key = (xname or tok).lower()
            extra.setdefault(key, xname or tok)
            spellings[key][tok] += unknown[tok]
        resolved[op] = key
    return resolved, spellings, extra, conflicts


def build(wiki, scan, xedit, notes):
    canon = canonical_map(wiki, xedit)
    xedit_by_name = {name.lower(): (idx, params) for idx, (name, params) in xedit.items()}
    resolved, spellings, extra, conflicts = resolve_opcodes(scan, canon, xedit)
    observed = collections.defaultdict(dict)    # function key -> {opcode: "observed"}
    for op, key in resolved.items():
        observed[key][op] = "observed"

    entries = [dict(rec) for rec in wiki]
    wiki_keys = {rec["name"].lower() for rec in wiki}
    entries += [dict(rec) for rec in PAGELESS if rec["name"].lower() not in wiki_keys]
    wiki_keys |= {rec["name"].lower() for rec in PAGELESS}
    xedit_names = {name.lower(): name for name, _ in xedit.values()}
    for key in observed:
        if key not in extra:
            extra[key] = xedit_names.get(key) or spellings[key].most_common(1)[0][0]
    for key, shown in sorted(extra.items()):
        if key not in wiki_keys:
            entries.append({"name": shown, "page": None, "origin": "observed-only",
                            "categories": []})

    anomalies = []
    for e in entries:
        key = e["name"].lower()
        vanilla = e["origin"] in VANILLA_ORIGINS or e["origin"] == "observed-only"
        e["family"] = "vanilla" if vanilla else "extender"
        if not vanilla and core_family(e):
            e["core_family"] = core_family(e)
        if not vanilla and jip_family(e):
            e["jip_family"] = jip_family(e)
            e["jip_probe"] = jip_probe(e)
        if e.get("page"):
            e["wiki"] = WIKI_URL + e["page"].replace(" ", "_")
        ops = observed.get(key, {})
        xi = xedit_by_name.get(key)
        if xi and xi[0] < 0x1000 and vanilla:
            e["condition_index"] = xi[0]
            e["xedit_param_types"] = xi[1]
        if ops:
            e["opcodes"] = [f"0x{o:04X}" for o in sorted(ops)]
            e["opcode_evidence"] = "observed"
            if len(ops) > 1:
                anomalies.append({"function": e["name"], "opcodes": e["opcodes"]})
            if xi and xi[0] + 0x1000 not in ops:
                anomalies.append({"function": e["name"], "opcodes": e["opcodes"],
                                  "xedit_index": xi[0]})
        elif xi and vanilla and xi[0] < 0x1000:
            e["opcodes"] = [f"0x{xi[0] + 0x1000:04X}"]
            e["opcode_evidence"] = "xedit_index"
        if ops:
            e["script_calls"] = sum(scan["calls"][o] for o in ops)
            e["scripts_using"] = sum(scan["scripts"].get(o, 0) for o in ops)
            documented = {key} | {a.lower() for a in e.get("aliases", [])}
            seen = set()
            sp = []
            for s, _ in spellings[key].most_common():
                if s.lower() not in documented and s.lower() not in seen:
                    seen.add(s.lower())
                    sp.append(s)
            if sp:
                e["observed_spellings"] = sp
        if xi and xi[0] in scan["ctda"]:
            e["condition_uses"] = scan["ctda"][xi[0]]
        n = notes.get(e.get("page") or e["name"])
        if n:
            for k in ("summary", "notes", "bugs"):
                if n.get(k):
                    e[k] = n[k]
    order = {"vanilla": 0, "extender": 1}
    entries.sort(key=lambda e: (order[e["family"]], e["name"].lower()))
    begin = [{"code": f"0x{c:04X}", "block": names.most_common(1)[0][0],
              "scripts": sum(names.values())}
             for c, names in sorted(scan["begin"].items())]
    stmts = [{"opcode": f"0x{o:04X}", "statement": STATEMENT_OPCODES.get(o, "?"),
              "count": c} for o, c in sorted(scan["statements"].items()) if o < 0x1000]
    return {"schema": SCHEMA,
            "sources": {
                "wiki": "GECK wiki, Category:Functions (All), Special:Export (structured "
                        "template fields only; prose paraphrased in notes.json)",
                "masters": MASTERS,
                "xedit": "TES5Edit/TES5Edit dev-4.1.6 Core/wbDefinitionsFNV.pas "
                         "(MPL-2.0), condition-function table"},
            "scan": dict(scan["stats"]),
            "statement_opcodes": stmts,
            "begin_blocks": begin,
            "opcode_conflicts": conflicts,
            "unresolved_opcodes": [{"opcode": f"0x{o:04X}", "calls": c}
                                   for o, c in sorted(scan["calls"].items())
                                   if o not in resolved],
            "anomalies": anomalies,
            "functions": entries}


def render_json(doc):
    """One list element per line, so a regenerated catalogue diffs line by line."""
    parts = []
    for key, value in doc.items():
        if isinstance(value, list) and value:
            items = ",\n".join("  " + json.dumps(v, ensure_ascii=False) for v in value)
            parts.append(f" {json.dumps(key)}: [\n{items}\n ]")
        else:
            parts.append(f" {json.dumps(key)}: {json.dumps(value, ensure_ascii=False)}")
    return "{\n" + ",\n".join(parts) + "\n}\n"


# --------------------------------------------------------------------------- markdown

def render_md(doc):
    fns = [f for f in doc["functions"] if f["family"] == "vanilla"]
    out = ["# Script function catalogue: base game (generated)", "",
           "Generated by `tools/capture/script_functions.py` from "
           "`schemas/script/functions.json`; do not edit by hand. Conventions and tags: "
           "`docs/spec-notes/script-functions.md`.", "",
           "Columns: opcode (`obs` = observed in shipped bytecode, `xe` = xEdit condition "
           "index + 0x1000), calls / scripts = uses in the shipped masters' compiled scripts, "
           "cond = uses in CTDA conditions, params (`?` = optional).", ""]
    by_cat = collections.defaultdict(list)
    for f in fns:
        cats = [c for c in f.get("categories", []) if c not in (
            "Functions", "Functions (GECK)", "Functions (FO3)", "Functions (Vegas)",
            "Condition Functions", "Console Functions", "Deprecated Functions",
            "Savebaked Functions", "Engine Differences (Fallout)", "Conditions",
            "Unused Functions", "Functions (Consoles)")]
        by_cat[cats[0] if cats else "Uncategorised"].append(f)
    for cat in sorted(by_cat):
        out += [f"## {cat}", "",
                "| Function | Opcode | Calls / scripts | Cond | Params | Returns | Summary |",
                "|---|---|---|---|---|---|---|"]
        for f in sorted(by_cat[cat], key=lambda f: f["name"].lower()):
            name = f"[{f['name']}]({f['wiki']})" if f.get("wiki") else f["name"]
            if f.get("aliases"):
                name += " (" + ", ".join(f["aliases"]) + ")"
            if f.get("calling_reference"):
                name = "ref." + name
            op = ""
            if f.get("opcodes"):
                op = " ".join(f["opcodes"]) + (" obs" if f["opcode_evidence"] == "observed"
                                               else " xe")
            use = f"{f['script_calls']} / {f['scripts_using']}" if f.get("script_calls") else ""
            params = ", ".join(p["name"] + ("?" if p.get("optional") else "") + ":" + p["kind"]
                               for p in f.get("params", []))
            ret = (f.get("returns") or {}).get("type") or ""
            summ = (f.get("summary") or "").replace("|", "/")
            out.append(f"| {name} | {op} | {use} | {f.get('condition_uses', '')} | "
                       f"{params.replace('|', '/')} | {ret.replace('|', '/')} | {summ} |")
        out.append("")
    return "\n".join(out) + "\n"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--wiki", required=True)
    ap.add_argument("--data", required=True)
    ap.add_argument("--xedit")
    ap.add_argument("--xedit-from-catalogue", metavar="FUNCTIONS_JSON",
                    help="take the xEdit table from an earlier functions.json instead")
    ap.add_argument("--notes")
    ap.add_argument("--out", default="schemas/script/functions.json")
    ap.add_argument("--out-md", default="docs/spec-notes/script-functions-catalogue.md")
    args = ap.parse_args(argv)
    try:
        wiki = parse_wiki(args.wiki)
        if args.xedit:
            xedit = parse_xedit(args.xedit)
        elif args.xedit_from_catalogue:
            xedit = xedit_from_catalogue(args.xedit_from_catalogue)
        else:
            xedit = {}
        notes = {}
        if args.notes:
            with open(args.notes, encoding="utf-8") as fh:
                notes = {n["page"]: n for n in json.load(fh)["functions"]}
    except (OSError, ET.ParseError, ValueError) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1
    scan = scan_masters(args.data, canonical_map(wiki, xedit))
    doc = build(wiki, scan, xedit, notes)
    with open(args.out, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(render_json(doc))
    with open(args.out_md, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(render_md(doc))
    fam = collections.Counter(f["family"] for f in doc["functions"])
    print(json.dumps({"functions": dict(fam), "scan": doc["scan"],
                      "with_opcode": sum(1 for f in doc["functions"] if f.get("opcodes")),
                      "conflicts": len(doc["opcode_conflicts"]),
                      "anomalies": doc["anomalies"]}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
