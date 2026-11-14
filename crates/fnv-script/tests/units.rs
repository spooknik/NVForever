//! Script units read from synthetic records (no game data): owners of every embedding
//! record type, variables and references through the Layer 1 schema, and remote
//! variable names seen by the plugin that compiled the source vs. by the load order.

use fnv_esm::raw::write_subrecords;
use fnv_esm::{LoadOrder, Plugin, Schema, Subrecord};
use fnv_script::resolve::{ScriptIndex, Viewer};
use fnv_script::unit::{read_load_order, PackageEvent, RefSlot, UnitOwner};
use fnv_script::{decode, decompile::Decompiler, FunctionTable, ScriptKind, VarType};

fn schema() -> Schema {
    Schema::load_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv")).unwrap()
}

fn table() -> FunctionTable {
    FunctionTable::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json"))
        .unwrap()
}

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

fn z(s: &str) -> Vec<u8> {
    [s.as_bytes(), &[0]].concat()
}

fn record(sig: &[u8; 4], fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [
        &sig[..],
        &(payload.len() as u32).to_le_bytes(),
        &0u32.to_le_bytes(),
        &fid.to_le_bytes(),
        &[0; 4],
        &15u16.to_le_bytes(),
        &[0; 2],
        &payload,
    ]
    .concat()
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [b"GRUP", &((24 + body.len()) as u32).to_le_bytes()[..], sig, &0i32.to_le_bytes(), &[0; 8], body].concat()
}

fn tes4(masters: &[&str]) -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    let mut subs = vec![sub(b"HEDR", hedr)];
    for m in masters {
        subs.push(sub(b"MAST", z(m)));
        subs.push(sub(b"DATA", vec![0; 8]));
    }
    let payload = write_subrecords(&subs);
    [&b"TES4"[..], &(payload.len() as u32).to_le_bytes(), &1u32.to_le_bytes(), &[0; 12], &payload].concat()
}

/// SCHR + SCDA + SCTX + variables + references.
fn unit(kind: u16, scda: &[u8], src: &str, vars: &[(u32, &str, u8)], refs: &[(bool, u32)]) -> Vec<Subrecord> {
    let mut schr = vec![0u8; 4];
    schr.extend((refs.len() as u32).to_le_bytes());
    schr.extend((scda.len() as u32).to_le_bytes());
    schr.extend((vars.len() as u32).to_le_bytes());
    schr.extend(kind.to_le_bytes());
    schr.extend(1u16.to_le_bytes());
    let mut out = vec![sub(b"SCHR", schr)];
    if !scda.is_empty() {
        out.push(sub(b"SCDA", scda.to_vec()));
    }
    out.push(sub(b"SCTX", src.as_bytes().to_vec()));
    for (i, n, t) in vars {
        let mut slsd = i.to_le_bytes().to_vec();
        slsd.extend([0u8; 12]);
        slsd.push(*t);
        slsd.extend([0u8; 7]);
        out.push(sub(b"SLSD", slsd));
        out.push(sub(b"SCVR", z(n)));
    }
    for (form, v) in refs {
        out.push(sub(if *form { b"SCRO" } else { b"SCRV" }, v.to_le_bytes().to_vec()));
    }
    out
}

const QUEST: u32 = 0x0000_0900;
const QUEST_SCRIPT: u32 = 0x0000_0901;
const OTHER_SCRIPT: u32 = 0x0000_0902;

/// `set Quest.<var 2> to 1` (an `r` + `s` Set target).
fn set_remote() -> Vec<u8> {
    let target = [b'r', 1, 0, b's', 2, 0];
    let e = b" 1";
    [&0x15u16.to_le_bytes()[..], &((target.len() + 2 + e.len()) as u16).to_le_bytes(), &target, &(e.len() as u16).to_le_bytes(), e]
        .concat()
}

fn base_plugin() -> Vec<u8> {
    let scpt = |fid, name: &str, vars: &[(u32, &str, u8)]| {
        let mut s = vec![sub(b"EDID", z(name))];
        s.extend(unit(1, &[0x1D, 0, 0, 0], &format!("scn {name}"), vars, &[]));
        record(b"SCPT", fid, &s)
    };
    let scripts = [
        scpt(QUEST_SCRIPT, "QScript", &[(1, "fTimer", 0), (2, "bDone", 1)]),
        // Uses the quest's variable 2 from a standalone script.
        record(
            b"SCPT",
            OTHER_SCRIPT,
            &[
                vec![sub(b"EDID", z("Other"))],
                unit(0, &[[0x1D, 0, 0, 0].as_slice(), &set_remote()].concat(), "scn Other\nset Q.bDone to 1", &[], &[(true, QUEST)]),
            ]
            .concat(),
        ),
    ]
    .concat();
    let mut qust = vec![sub(b"EDID", z("Q")), sub(b"SCRI", QUEST_SCRIPT.to_le_bytes().to_vec())];
    qust.push(sub(b"INDX", 10i16.to_le_bytes().to_vec()));
    qust.push(sub(b"QSDT", vec![0]));
    qust.extend(unit(0, &set_remote(), "set Q.bDone to 1", &[], &[(true, QUEST)]));
    qust.push(sub(b"QSDT", vec![0]));
    qust.extend(unit(0, &[], "; nothing", &[], &[]));
    qust.push(sub(b"INDX", 20i16.to_le_bytes().to_vec()));
    qust.push(sub(b"QSDT", vec![1]));
    qust.extend(unit(0, &[], "", &[], &[]));
    let mut info = unit(0, &[], "", &[], &[]);
    info.push(sub(b"NEXT", vec![]));
    info.extend(unit(0, &set_remote(), "set Q.bDone to 1", &[], &[(true, QUEST)]));
    let mut pack = vec![sub(b"EDID", z("P"))];
    for m in [b"POBA", b"POEA", b"POCA"] {
        pack.push(sub(m, vec![]));
        pack.push(sub(b"INAM", vec![0; 4]));
        pack.extend(unit(0, &[], "", &[], &[]));
        pack.push(sub(b"TNAM", vec![0; 4]));
    }
    let mut term = vec![sub(b"EDID", z("T"))];
    for i in 0..2 {
        term.push(sub(b"ITXT", z(&format!("item {i}"))));
        term.extend(unit(0, &[], "", &[], &[(false, 1)]));
    }
    let mut perk = vec![sub(b"EDID", z("K")), sub(b"PRKE", vec![2, 0, 0])];
    perk.extend(unit(0, &[], "", &[], &[]));
    let mut refr = vec![sub(b"NAME", 0x20u32.to_le_bytes().to_vec()), sub(b"XPRD", vec![0; 4]), sub(b"XPPA", vec![])];
    refr.extend(unit(0, &[], "", &[], &[]));
    [
        tes4(&[]),
        top(b"SCPT", &scripts),
        top(b"QUST", &record(b"QUST", QUEST, &qust)),
        top(b"INFO", &record(b"INFO", 0x0910, &info)),
        top(b"PACK", &record(b"PACK", 0x0920, &pack)),
        top(b"TERM", &record(b"TERM", 0x0930, &term)),
        top(b"PERK", &record(b"PERK", 0x0940, &perk)),
        top(b"REFR", &record(b"REFR", 0x0950, &refr)),
    ]
    .concat()
}

/// A later plugin overrides the quest script and renames variable 2.
fn dlc_plugin() -> Vec<u8> {
    let mut s = vec![sub(b"EDID", z("QScript"))];
    s.extend(unit(1, &[0x1D, 0, 0, 0], "scn QScript", &[(1, "fTimer", 0), (2, "bRenamed", 1)], &[]));
    [tes4(&["Base.esm"]), top(b"SCPT", &record(b"SCPT", QUEST_SCRIPT, &s))].concat()
}

#[test]
fn owners_variables_and_references() {
    let plugins = vec![Plugin::parse(&base_plugin(), "Base.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = schema();
    let (units, issues) = read_load_order(&lo, &schema);
    assert!(issues.is_empty(), "{issues:?}");
    let labels: Vec<String> = units.iter().map(|u| u.key()).collect();
    assert_eq!(
        labels,
        [
            "Base.esm|SCPT 0x00000901",
            "Base.esm|SCPT 0x00000902",
            "Base.esm|QUST 0x00000900 stage 10 entry 0",
            "Base.esm|QUST 0x00000900 stage 10 entry 1",
            "Base.esm|QUST 0x00000900 stage 20 entry 0",
            "Base.esm|INFO 0x00000910 begin",
            "Base.esm|INFO 0x00000910 end",
            "Base.esm|PACK 0x00000920 begin",
            "Base.esm|PACK 0x00000920 end",
            "Base.esm|PACK 0x00000920 change",
            "Base.esm|TERM 0x00000930 item 0",
            "Base.esm|TERM 0x00000930 item 1",
            "Base.esm|PERK 0x00000940 entry 0",
            "Base.esm|REFR 0x00000950 patrol",
        ]
    );
    let q = &units[0];
    assert_eq!(q.kind, ScriptKind::Quest);
    assert_eq!(q.variables[1].name, "bDone");
    assert_eq!(q.variables[1].ty, VarType::Int);
    assert_eq!(q.variables[0].ty, VarType::FloatOrRef);
    assert!(q.has_scda && q.source.as_deref() == Some("scn QScript"));
    assert_eq!(units[2].refs, vec![RefSlot::Form(QUEST)]);
    assert!(matches!(units[8].owner, UnitOwner::Package { event: PackageEvent::End, .. }));
    assert_eq!(units[10].refs, vec![RefSlot::Local(1)]);
    assert!(!units[3].has_scda);
}

#[test]
fn remote_variables_follow_the_viewer() {
    let plugins = vec![
        Plugin::parse(&base_plugin(), "Base.esm".into()).unwrap(),
        Plugin::parse(&dlc_plugin(), "Dlc.esm".into()).unwrap(),
    ];
    let lo = LoadOrder::new(&plugins).unwrap();
    let schema = schema();
    let (units, _) = read_load_order(&lo, &schema);
    let idx = ScriptIndex::build(&lo, &schema, &units);
    assert_eq!(idx.script_of(QUEST, Viewer::LoadOrder), Some(QUEST_SCRIPT));
    assert_eq!(idx.remote_var(QUEST, 2, Viewer::Plugin(0)).map(|v| v.name.as_str()), Some("bDone"));
    assert_eq!(idx.remote_var(QUEST, 2, Viewer::LoadOrder).map(|v| v.name.as_str()), Some("bRenamed"));
    // The stage script of Base.esm prints the name it was compiled against.
    let t = table();
    let stage = units.iter().find(|u| u.key() == "Base.esm|QUST 0x00000900 stage 10 entry 0").unwrap();
    let d = decode(&stage.bytes, &t);
    assert_eq!(d.report.raw_total(), 0);
    let text = Decompiler { unit: stage, table: &t, names: &idx }.lines(&d.program);
    assert_eq!(text, ["set 0x00000900.bdone to 1"]);
}

/// A plugin that holds two records under one FormID (oHUD.esm's QUST `aHUD` and MESG
/// `iHUDMainMenu`, both 0x01000801): the quest's name and its script's variables still
/// resolve, the way the editor resolved them when it compiled the plugin's scripts
/// [observed: oHUD's shipped bytes name that FormID and read its quest script's variable].
#[test]
fn a_name_resolves_when_another_record_shares_its_formid() {
    use fnv_script::compile::{compile, env::LoadOrderIndex};
    let quest_script = record(b"SCPT", 0x802, &[&[sub(b"EDID", z("QScript"))][..], &unit(1, &[], "scn QScript\nfloat mcmVersion\n", &[(19, "mcmVersion", 0)], &[])].concat());
    let quest = record(b"QUST", 0x801, &[sub(b"EDID", z("aHUD")), sub(b"SCRI", 0x802u32.to_le_bytes().to_vec())]);
    let message = record(b"MESG", 0x801, &[sub(b"EDID", z("iHUDMainMenu"))]);
    let bytes = [tes4(&[]), top(b"SCPT", &quest_script), top(b"QUST", &quest), top(b"MESG", &message)].concat();
    let plugins = vec![Plugin::parse(&bytes, "O.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let (units, _) = read_load_order(&lo, &schema());
    let ix = LoadOrderIndex::build(&lo, &units);
    let c = compile("scn T\nbegin GameMode\nset aHUD.mcmVersion to 5\nend\n", &table(), &ix.env(0), None);
    assert_eq!(c.errors().count(), 0, "{:?}", c.diags);
    assert_eq!(c.refs, [RefSlot::Form(0x801)]);
}

#[test]
fn serialized_extender_variable_kinds_survive_the_unit_reader() {
    let mut fields = vec![sub(b"EDID", z("OwnedKinds"))];
    fields.extend(unit(0, &[], "scn OwnedKinds", &[(1, "textSlot", 2), (2, "arraySlot", 3), (3, "refSlot", 4), (4, "unknownSlot", 200)], &[]));
    let bytes = [tes4(&[]), top(b"SCPT", &record(b"SCPT", 0x900, &fields))].concat();
    let plugins = vec![Plugin::parse(&bytes, "Owned.esm".into()).unwrap()];
    let lo = LoadOrder::new(&plugins).unwrap();
    let (units, issues) = read_load_order(&lo, &schema());
    assert!(issues.is_empty(), "{issues:?}");
    let variables = &units[0].variables;
    assert_eq!(variables[0].ty, VarType::Other("string_var".into()));
    assert_eq!(variables[1].ty, VarType::Other("array_var".into()));
    assert_eq!(variables[2].ty, VarType::FloatOrRef);
    assert_eq!(variables[3].ty, VarType::Other("200".into()));
    assert_eq!(variables.iter().map(|v| v.index).collect::<Vec<_>>(), [1, 2, 3, 4]);
    assert_eq!(variables.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["textSlot", "arraySlot", "refSlot", "unknownSlot"]);
}
