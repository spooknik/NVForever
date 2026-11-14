//! Integration tests on synthetic plugins built in memory (no game data).

use fnv_esm::decode::{decode_record, Coverage};
use fnv_esm::loadorder::LoadOrderError;
use fnv_esm::raw::{visit_records, write_subrecords, Subrecord};
use fnv_esm::{LoadOrder, Plugin, Schema};

fn sub(sig: &[u8; 4], data: &[u8]) -> Subrecord {
    Subrecord { sig: *sig, data: data.to_vec(), extended: false }
}

fn record(sig: &[u8; 4], flags: u32, fid: u32, payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(sig);
    v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    v.extend_from_slice(&flags.to_le_bytes());
    v.extend_from_slice(&fid.to_le_bytes());
    v.extend_from_slice(&0u32.to_le_bytes());
    v.extend_from_slice(&15u16.to_le_bytes());
    v.extend_from_slice(&0u16.to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn group(label: &[u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"GRUP");
    v.extend_from_slice(&((24 + body.len()) as u32).to_le_bytes());
    v.extend_from_slice(label);
    v.extend_from_slice(&gtype.to_le_bytes());
    v.extend_from_slice(&[0u8; 8]);
    v.extend_from_slice(body);
    v
}

fn tes4(masters: &[&str], num_records: i32, onam: &[u32]) -> Vec<u8> {
    let mut subs = vec![sub(b"HEDR", &{
        let mut h = Vec::new();
        h.extend_from_slice(&1.34f32.to_le_bytes());
        h.extend_from_slice(&num_records.to_le_bytes());
        h.extend_from_slice(&0x800u32.to_le_bytes());
        h
    })];
    for m in masters {
        let mut n = m.as_bytes().to_vec();
        n.push(0);
        subs.push(sub(b"MAST", &n));
        subs.push(sub(b"DATA", &[0; 8]));
    }
    if !onam.is_empty() {
        let d: Vec<u8> = onam.iter().flat_map(|x| x.to_le_bytes()).collect();
        subs.push(sub(b"ONAM", &d));
    }
    record(b"TES4", if masters.is_empty() { 1 } else { 0 }, 0, &write_subrecords(&subs))
}

fn glob(fid: u32, edid: &str, value: f32) -> Vec<u8> {
    let mut e = edid.as_bytes().to_vec();
    e.push(0);
    record(
        b"GLOB",
        0,
        fid,
        &write_subrecords(&[sub(b"EDID", &e), sub(b"FNAM", b"f"), sub(b"FLTV", &value.to_le_bytes())]),
    )
}

fn master() -> Vec<u8> {
    let body = [glob(0x0000_1000, "GlobA", 1.0), glob(0x0000_1001, "GlobB", 2.0)].concat();
    let mut f = tes4(&[], 3, &[]);
    f.extend(group(b"GLOB", 0, &body));
    f
}

fn dlc() -> Vec<u8> {
    // Overrides GlobB, adds GlobC; GlobC is compressed.
    let payload = write_subrecords(&[sub(b"EDID", b"GlobC\0"), sub(b"FNAM", b"f"), sub(b"FLTV", &3.5f32.to_le_bytes())]);
    let mut comp = (payload.len() as u32).to_le_bytes().to_vec();
    comp.extend(fnv_inflate::zlib_store(&payload));
    let body = [glob(0x0000_1001, "GlobB", 20.0), record(b"GLOB", 0x40000, 0x0100_2000, &comp)].concat();
    let mut f = tes4(&["Master.esm"], 3, &[]);
    f.extend(group(b"GLOB", 0, &body));
    f
}

const SCHEMA: &str = r#"{"format":"fnv.schema/1",
  "common":{"EDID":{"name":"Editor ID","type":"zstring","confidence":"observed"}},
  "records":{"GLOB":{"name":"Global","subrecords":{
     "FNAM":{"name":"Type","type":{"enum":"u8","values":{"102":"Float","115":"Short","108":"Long"}}},
     "FLTV":{"name":"Value","type":"f32"}}}}}"#;

fn schema() -> Schema {
    let mut s = Schema::default();
    s.merge_str(SCHEMA, "test").unwrap();
    s.validate().unwrap();
    s
}

#[test]
fn parse_header_and_roundtrip() {
    let bytes = dlc();
    let p = Plugin::parse(&bytes, "Dlc.esm".into()).unwrap();
    assert_eq!(p.header.masters.len(), 1);
    assert_eq!(p.header.masters[0].name, "Master.esm");
    assert_eq!(p.header.num_records, 3);
    assert!(!p.header.is_master());
    assert_eq!(p.to_bytes(), bytes);
}

#[test]
fn load_order_overrides_and_remap() {
    let plugins = vec![
        Plugin::parse(&master(), "Master.esm".into()).unwrap(),
        Plugin::parse(&dlc(), "Dlc.esm".into()).unwrap(),
    ];
    let lo = LoadOrder::new(&plugins).unwrap();
    // DLC-local 0x01002000 is global 0x01002000 (DLC at load index 1).
    assert_eq!(lo.to_global(1, 0x0100_2000), Some(0x0100_2000));
    // DLC-local 0x00001001 refers to the master.
    assert_eq!(lo.to_global(1, 0x0000_1001), Some(0x0000_1001));
    // An index beyond the master list means the plugin itself, as in the game.
    assert_eq!(lo.to_global(1, 0x0200_0001), Some(0x0100_0001));
    assert_eq!(lo.to_global(1, 0x1200_0001), Some(0x0100_0001));
    assert_eq!(lo.to_local(1, 0x0000_1001), Some(0x0000_1001));
    assert_eq!(lo.to_local(0, 0x0100_2000), None);
    let (wp, wr) = lo.winner(0x0000_1001).unwrap();
    assert_eq!(wp.name, "Dlc.esm");
    let decoded = decode_record(wr, &schema());
    let s = decoded.to_string();
    assert!(s.contains("20.0"), "{s}");
    assert_eq!(lo.index.len(), 3);
    assert_eq!(lo.index[&0x0000_1001].len(), 2);
}

/// Two records under one FormID in one plugin: the first is the form [observed: oHUD.esm's
/// QUST `aHUD` before its MESG `iHUDMainMenu`, #2276 job B]; a later plugin still
/// overrides it.
#[test]
fn a_plugins_first_record_under_a_shared_formid_wins() {
    let mut own = tes4(&[], 2, &[]);
    own.extend(group(b"GLOB", 0, &[glob(0x0000_1000, "First", 1.0), glob(0x0000_1000, "Second", 2.0)].concat()));
    let mut patch = tes4(&["Own.esm"], 1, &[]);
    patch.extend(group(b"GLOB", 0, &glob(0x0000_1000, "Patched", 3.0)));
    let one = vec![Plugin::parse(&own, "Own.esm".into()).unwrap()];
    let lo = LoadOrder::new(&one).unwrap();
    assert_eq!(lo.index[&0x0000_1000].len(), 1);
    let (_, r) = lo.winner(0x0000_1000).unwrap();
    assert!(decode_record(r, &schema()).to_string().contains("First"));
    let both = vec![Plugin::parse(&own, "Own.esm".into()).unwrap(), Plugin::parse(&patch, "Patch.esp".into()).unwrap()];
    let lo = LoadOrder::new(&both).unwrap();
    assert_eq!(lo.index[&0x0000_1000].len(), 2);
    assert!(decode_record(lo.winner(0x0000_1000).unwrap().1, &schema()).to_string().contains("Patched"));
}

#[test]
fn load_order_errors() {
    let d = Plugin::parse(&dlc(), "Dlc.esm".into()).unwrap();
    let m = Plugin::parse(&master(), "Master.esm".into()).unwrap();
    let only_dlc = vec![Plugin::parse(&dlc(), "Dlc.esm".into()).unwrap()];
    assert!(matches!(LoadOrder::new(&only_dlc), Err(LoadOrderError::MissingMaster { .. })));
    let wrong = vec![d, m];
    assert!(matches!(LoadOrder::new(&wrong), Err(LoadOrderError::MasterAfterDependent { .. })));
}

#[test]
fn compressed_record_decodes_and_coverage_counts() {
    let p = Plugin::parse(&dlc(), "Dlc.esm".into()).unwrap();
    let s = schema();
    let mut cov = Coverage::default();
    let mut n = 0;
    visit_records(&p.nodes[1..], &mut |r, _| {
        n += 1;
        cov.add_record(r, &s, None);
        if r.is_compressed() {
            let v = decode_record(r, &s);
            let t = v.to_string();
            assert!(t.contains("GlobC") && t.contains("3.5") && t.contains("Float"), "{t}");
        }
    });
    assert_eq!(n, 2);
    let (total, ok, unknown, trailing, error) = cov.totals();
    assert_eq!((total, ok, unknown, trailing, error), (6, 6, 0, 0, 0));
}

#[test]
fn unknown_and_trailing_are_reported() {
    let r = record(
        b"GLOB",
        0,
        0x10,
        &write_subrecords(&[sub(b"EDID", b"X\0"), sub(b"FLTV", &[0, 0, 128, 63, 9, 9]), sub(b"ZZZZ", &[1])]),
    );
    let mut f = tes4(&[], 2, &[]);
    f.extend(group(b"GLOB", 0, &r));
    let p = Plugin::parse(&f, "T.esm".into()).unwrap();
    let mut cov = Coverage::default();
    let s = schema();
    visit_records(&p.nodes[1..], &mut |r, _| cov.add_record(r, &s, None));
    let (total, ok, unknown, trailing, error) = cov.totals();
    assert_eq!((total, ok, unknown, trailing, error), (3, 1, 1, 1, 0));
}

#[test]
fn rejects_non_plugin() {
    assert!(Plugin::parse(b"not a plugin file at all, nope....", "x".into()).is_err());
    // A group first instead of TES4.
    let g = group(b"GLOB", 0, &glob(1, "A", 1.0));
    assert!(Plugin::parse(&g, "x".into()).is_err());
}

#[test]
fn repository_schemas_load() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv");
    let s = Schema::load_dir(&dir).unwrap_or_else(|e| panic!("{e}"));
    assert!(s.records.len() >= 30, "only {} record definitions", s.records.len());
    assert!(s.common.contains_key(b"EDID"));
}

#[test]
fn stats_counts_repeated_unknown_group_types() {
    let mut f = tes4(&[], 4, &[]);
    for _ in 0..3 {
        f.extend(group(b"XXXX", 11, &[]));
    }
    let p = Plugin::parse(&f, "G.esm".into()).unwrap();
    let v = fnv_esm::inventory::stats_json(&p);
    let g = v.get("by_group_type").unwrap();
    assert_eq!(g.get("unknown_11").and_then(|x| x.as_u64()), Some(3));
    assert_eq!(g.as_object().unwrap().iter().filter(|(k, _)| *k == "unknown_11").count(), 1);
}

#[test]
fn schema_cannot_request_unbounded_zero_width_arrays() {
    let mut s = Schema::default();
    s.merge_str(
        r#"{"format":"fnv.schema/1","records":{"GLOB":{"name":"G","subrecords":{
            "FLTV":{"name":"bad","type":{"array":{"struct":[]},"count":100000000000}}}}}}"#,
        "t",
    )
    .unwrap();
    s.validate().unwrap();
    let r = record(b"GLOB", 0, 1, &write_subrecords(&[sub(b"FLTV", &[1, 2, 3, 4])]));
    let mut f = tes4(&[], 2, &[]);
    f.extend(group(b"GLOB", 0, &r));
    let p = Plugin::parse(&f, "T.esm".into()).unwrap();
    let mut cov = Coverage::default();
    visit_records(&p.nodes[1..], &mut |r, _| cov.add_record(r, &s, None));
    assert_eq!(cov.totals().4, 1, "must be reported as an error");
}

#[test]
fn zstring_array_lists_and_terminator() {
    let mut s = Schema::default();
    s.merge_str(
        r#"{"format":"fnv.schema/1","records":{"CREA":{"name":"C","subrecords":{
            "KFFZ":{"name":"Animations","type":"zstring_array"}}}}}"#,
        "t",
    )
    .unwrap();
    s.validate().unwrap();
    let cases: [(&[u8], Option<&str>); 5] = [
        (b"Aim.kf\0b.kf\0\0", Some(r#"["Aim.kf","b.kf"]"#)),
        (b"a.kf\0", Some(r#"["a.kf"]"#)),
        (b"\0", Some("[]")),
        (b"\0\0", Some("[]")),
        (b"a.kf\0b.kf", None),
    ];
    for (i, (data, want)) in cases.iter().enumerate() {
        let r = record(b"CREA", 0, 0x10 + i as u32, &write_subrecords(&[sub(b"KFFZ", data)]));
        let mut f = tes4(&[], 2, &[]);
        f.extend(group(b"CREA", 0, &r));
        let p = Plugin::parse(&f, "T.esm".into()).unwrap();
        let mut cov = Coverage::default();
        visit_records(&p.nodes[1..], &mut |r, _| cov.add_record(r, &s, None));
        let (_, ok, _, _, error) = cov.totals();
        match want {
            Some(w) => {
                assert_eq!((ok, error), (1, 0), "case {i}");
                let v = decode_record(p.nodes[1..].iter().find_map(first_record).unwrap(), &s).to_string();
                assert!(v.contains(w), "case {i}: {v}");
            }
            None => assert_eq!(error, 1, "case {i}: an unterminated list is an error"),
        }
    }
}

fn first_record(n: &fnv_esm::raw::Node) -> Option<&fnv_esm::raw::Record> {
    match n {
        fnv_esm::raw::Node::Record(r) => Some(r),
        fnv_esm::raw::Node::Group(g) => g.children.iter().find_map(first_record),
    }
}

/// Malformed-input properties: parsing and decoding never panic, and whatever
/// parses is written back byte-for-byte.
mod malformed {
    use super::*;
    use proptest::prelude::*;
    use proptest::sample::Index;

    fn real_schema() -> Schema {
        Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap()
    }

    fn exercise(p: &Plugin, schema: &Schema) {
        let mut cov = Coverage::default();
        visit_records(&p.nodes, &mut |r, _| {
            let _ = r.payload_checked();
            let _ = decode_record(r, schema);
            cov.add_record(r, schema, Some(&mut |_| {}));
        });
        let _ = cov.to_json(10);
        let _ = fnv_esm::inventory::inventory_json(p);
        let _ = fnv_esm::inventory::stats_json(p);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn arbitrary_bytes(data in prop::collection::vec(any::<u8>(), 0..600)) {
            if let Ok(p) = Plugin::parse(&data, "x.esp".into()) {
                prop_assert_eq!(p.to_bytes(), data);
            }
        }

        #[test]
        fn mutated_plugins(
            edits in prop::collection::vec((any::<Index>(), any::<u8>()), 1..6),
            cut in prop::option::of(any::<Index>()),
        ) {
            let schema = real_schema();
            let mut f = dlc();
            for (i, b) in &edits {
                let at = i.index(f.len());
                f[at] = *b;
            }
            if let Some(c) = cut {
                f.truncate(c.index(f.len() + 1));
            }
            if let Ok(p) = Plugin::parse(&f, "Dlc.esm".into()) {
                prop_assert_eq!(p.to_bytes(), f);
                exercise(&p, &schema);
                let m = Plugin::parse(&master(), "Master.esm".into()).unwrap();
                let _ = LoadOrder::new(&[m, p]);
            }
        }

        #[test]
        fn subrecords_roundtrip(subs in prop::collection::vec((any::<[u8; 4]>(), prop::collection::vec(any::<u8>(), 0..80)), 0..8)) {
            let subs: Vec<Subrecord> = subs.into_iter().map(|(sig, data)| Subrecord { sig, data, extended: false }).collect();
            let bytes = write_subrecords(&subs);
            let back = fnv_esm::raw::parse_subrecords(&bytes).unwrap();
            prop_assert_eq!(write_subrecords(&back), bytes);
        }

        /// Every schema-defined subrecord of every record type, filled with random
        /// bytes of random length, decodes to a value or a reported error.
        #[test]
        fn schema_decode_random_payloads(rec in any::<Index>(), seed in prop::collection::vec(any::<u8>(), 0..64)) {
            let schema = real_schema();
            let mut sigs: Vec<&[u8; 4]> = schema.records.keys().collect();
            sigs.sort();
            let rsig = sigs[rec.index(sigs.len())];
            let mut subdefs: Vec<&[u8; 4]> = schema.records[rsig].subrecords.keys().collect();
            subdefs.sort();
            let subs: Vec<Subrecord> = subdefs
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let n = seed.get(i).map_or(i, |&b| b as usize % 48);
                    let data = (0..n).map(|k| seed.get((i + k) % seed.len().max(1)).copied().unwrap_or(k as u8)).collect();
                    Subrecord { sig: **s, data, extended: false }
                })
                .collect();
            let mut f = tes4(&[], 1, &[]);
            f.extend(group(rsig, 0, &record(rsig, 0, 0x800, &write_subrecords(&subs))));
            let p = Plugin::parse(&f, "S.esm".into()).unwrap();
            exercise(&p, &schema);
        }

        #[test]
        fn schema_text(text in ".{0,200}") {
            let _ = Schema::default().merge_str(&text, "fuzz");
        }
    }
}

#[test]
fn size_union_picks_variant_by_length() {
    use fnv_esm::decode::{decode_sub, DecodeCtx, SubStatus};
    let text = r#"{"format":"fnv.schema/1",
      "types":{"Old":{"struct":[{"name":"f","type":"f32"},{"name":"damage","type":"u16"}]}},
      "records":{"WATR":{"name":"Water","subrecords":{"DATA":{"name":"D","type":
        {"union":[{"size":2,"type":"u16"},{"size":6,"type":"Old"}]}}}},
        "TEST":{"name":"T","subrecords":{"DATA":{"name":"D","type":
        {"union":[{"size":1,"type":"u8"},{"type":"bytes"}]}}}}}}"#;
    let mut s = Schema::default();
    s.merge_str(text, "t").unwrap();
    s.validate().unwrap();
    let mut ctx = DecodeCtx { schema: &s, on_formid: None };
    let d = decode_sub(b"WATR", &sub(b"DATA", &[7, 0]), &mut ctx);
    assert_eq!((d.status, d.value.unwrap().to_string()), (SubStatus::Ok, "7".to_string()));
    let d = decode_sub(b"WATR", &sub(b"DATA", &[0, 0, 128, 63, 9, 0]), &mut ctx);
    assert_eq!(d.status, SubStatus::Ok);
    let v = d.value.unwrap();
    assert_eq!((v["damage"].as_u64(), v["f"].as_f64()), (Some(9), Some(1.0)));
    // No variant for 3 bytes and no fallback: an error, not a guess.
    assert_eq!(decode_sub(b"WATR", &sub(b"DATA", &[1, 2, 3]), &mut ctx).status, SubStatus::Error);
    // A variant without a size is the fallback.
    let d = decode_sub(b"TEST", &sub(b"DATA", &[1, 2, 3]), &mut ctx);
    assert_eq!((d.status, d.value.unwrap().to_string()), (SubStatus::Ok, "\"010203\"".to_string()));
    let bad = r#"{"format":"fnv.schema/1","common":{"DATA":{"type":{"union":[]}}}}"#;
    assert!(Schema::default().merge_str(bad, "x").is_err());
}

/// QUST DATA of 2 bytes (two FalloutNV.esm quests, #1001) decodes as flags and priority
/// with the real schema; the 8-byte form still reads its quest delay.
#[test]
fn quest_data_of_two_bytes_is_flags_and_priority() {
    use fnv_esm::decode::{decode_sub, DecodeCtx, SubStatus};
    let s = Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap();
    let mut ctx = DecodeCtx { schema: &s, on_formid: None };
    let d = decode_sub(b"QUST", &sub(b"DATA", &[5, 0]), &mut ctx);
    assert_eq!(d.status, SubStatus::Ok);
    let v = d.value.unwrap();
    assert_eq!((v["flags"]["value"].as_u64(), v["priority"].as_u64(), v.get("quest_delay")), (Some(5), Some(0), None));
    let d = decode_sub(b"QUST", &sub(b"DATA", &[1, 40, 0, 0, 0, 0, 128, 63]), &mut ctx);
    assert_eq!(d.status, SubStatus::Ok);
    let v = d.value.unwrap();
    assert_eq!((v["priority"].as_u64(), v["quest_delay"].as_f64()), (Some(40), Some(1.0)));
}
