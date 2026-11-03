//! Record inventory and structural statistics for one plugin. Schema-independent:
//! only the container structure and the EDID/FULL string subrecords are read, so
//! the inventory stays valid while schemas evolve.

use crate::decode::formid_str;
use crate::plugin::Plugin;
use crate::raw::{group_type, visit_groups, visit_records, Record};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub struct InventoryRow {
    pub form_id: u32,
    pub sig: String,
    pub flags: u32,
    pub form_version: u16,
    pub edid: Option<String>,
    pub full: Option<String>,
}

/// EDID and FULL of a record, if present. Errors (corrupt payload) yield `Err`.
pub fn names(r: &Record) -> Result<(Option<String>, Option<String>), String> {
    let subs = r.subrecords().map_err(|e| e.to_string())?;
    let mut edid = None;
    let mut full = None;
    for s in &subs {
        match &s.sig {
            b"EDID" if edid.is_none() => edid = Some(fnv_util::text::decode_z(&s.data)),
            b"FULL" if full.is_none() => full = Some(fnv_util::text::decode_z(&s.data)),
            _ => {}
        }
    }
    Ok((edid, full))
}

pub fn inventory(p: &Plugin) -> (Vec<InventoryRow>, Vec<String>) {
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    visit_records(&p.nodes[1..], &mut |r, _| {
        let (edid, full) = match names(r) {
            Ok(x) => x,
            Err(e) => {
                errors.push(e);
                (None, None)
            }
        };
        rows.push(InventoryRow {
            form_id: r.header.form_id,
            sig: r.sig_str(),
            flags: r.header.flags,
            form_version: r.header.form_version,
            edid,
            full,
        });
    });
    (rows, errors)
}

pub fn header_json(p: &Plugin) -> Value {
    let h = &p.header;
    let masters: Vec<&str> = h.masters.iter().map(|m| m.name.as_str()).collect();
    json!({
        "plugin": p.name,
        "is_master": h.is_master(),
        "flags": h.flags,
        "version": h.version as f64,
        "num_records": h.num_records,
        "next_object_id": formid_str(h.next_object_id),
        "author": h.author,
        "description": h.description,
        "masters": masters,
        "overridden_forms": h.overridden_forms.len(),
        "other_subrecords": h.other,
        "file_size": p.file_size,
    })
}

pub fn inventory_json(p: &Plugin) -> Value {
    let (rows, errors) = inventory(p);
    let recs: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "formid": formid_str(r.form_id),
                "type": r.sig,
                "edid": r.edid,
                "full": r.full,
                "flags": r.flags,
                "form_version": r.form_version,
            })
        })
        .collect();
    let errors: Vec<Value> = errors.into_iter().map(|e| json!({"code": "record_payload", "detail": e})).collect();
    json!({
        "schema": "fnv.esm.inventory/1",
        "plugin": p.name,
        "header": header_json(p),
        "errors": errors,
        "records": recs,
    })
}

pub fn stats_json(p: &Plugin) -> Value {
    let mut by_type: BTreeMap<String, u64> = BTreeMap::new();
    let mut compressed = 0u64;
    let mut deleted = 0u64;
    let mut records = 0u64;
    visit_records(&p.nodes, &mut |r, _| {
        records += 1;
        *by_type.entry(r.sig_str()).or_default() += 1;
        if r.is_compressed() {
            compressed += 1;
        }
        if r.header.flags & crate::raw::record_flags::DELETED != 0 {
            deleted += 1;
        }
    });
    let mut groups = 0u64;
    // All group-type names in numeric order, zero counts included (contract).
    let mut by_group: Vec<(String, u64)> =
        (0..=10).map(|t| (group_type::name(t).to_string(), 0u64)).collect();
    let mut top_labels = Vec::new();
    visit_groups(&p.nodes, &mut |g, depth| {
        groups += 1;
        let key = match group_type::name(g.header.group_type) {
            "unknown" => format!("unknown_{}", g.header.group_type),
            n => n.to_string(),
        };
        match by_group.iter_mut().find(|(n, _)| *n == key) {
            Some(e) => e.1 += 1,
            None => by_group.push((key, 1)),
        }
        if depth == 0 {
            top_labels.push(fnv_util::tag_str(&g.header.label));
        }
    });
    let hedr_matches = p.header.num_records as i64 == (records + groups) as i64 - 1;
    json!({
        "schema": "fnv.esm.stats/1",
        "plugin": p.name,
        "records": records,
        "groups": groups,
        "compressed_records": compressed,
        "deleted_records": deleted,
        "hedr_num_records": p.header.num_records,
        "hedr_matches_records_plus_groups_minus_one": hedr_matches,
        "top_groups": top_labels,
        "by_type": by_type,
        "by_group_type": by_group.into_iter().map(|(k, n)| (k, Value::from(n))).collect::<serde_json::Map<_, _>>(),
    })
}
